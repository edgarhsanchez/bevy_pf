//! WPF path mini-language parser (`Data="M 10,50 C 30,0 70,0 90,50 Z"`).
//!
//! The grammar is SVG-path compatible: uppercase commands take absolute
//! coordinates, lowercase relative; repeated coordinate sets repeat the
//! command; separators are whitespace and/or commas; an optional `F0`/`F1`
//! prefix selects the fill rule (EvenOdd default, like WPF).

use crate::error::{XamlError, XamlResult};
use crate::value::Point;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillRule {
    #[default]
    EvenOdd,
    NonZero,
}

/// A parsed geometry: one or more figures.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PathData {
    pub fill_rule: FillRule,
    pub figures: Vec<PathFigure>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathFigure {
    pub start: Point,
    pub segments: Vec<PathSegment>,
    pub closed: bool,
}

/// Segments with absolute coordinates (relative commands are normalized
/// during parsing; smooth commands have their control points computed).
#[derive(Debug, Clone, PartialEq)]
pub enum PathSegment {
    Line(Point),
    Cubic(Point, Point, Point),
    Quadratic(Point, Point),
    Arc {
        radii: Point,
        rotation: f32,
        large_arc: bool,
        sweep: bool,
        to: Point,
    },
}

impl PathData {
    /// Axis-aligned bounds of all control points (a fast, conservative
    /// approximation of the true geometry bounds, matching what layout needs).
    pub fn control_bounds(&self) -> Option<(Point, Point)> {
        let mut min = Point::new(f32::INFINITY, f32::INFINITY);
        let mut max = Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut any = false;
        let mut visit = |p: Point| {
            min.x = min.x.min(p.x);
            min.y = min.y.min(p.y);
            max.x = max.x.max(p.x);
            max.y = max.y.max(p.y);
            any = true;
        };
        for fig in &self.figures {
            visit(fig.start);
            for seg in &fig.segments {
                match seg {
                    PathSegment::Line(p) => visit(*p),
                    PathSegment::Cubic(a, b, c) => {
                        visit(*a);
                        visit(*b);
                        visit(*c);
                    }
                    PathSegment::Quadratic(a, b) => {
                        visit(*a);
                        visit(*b);
                    }
                    PathSegment::Arc { to, radii, .. } => {
                        visit(*to);
                        // conservative: include the radii box around the endpoint
                        visit(Point::new(to.x - radii.x, to.y - radii.y));
                        visit(Point::new(to.x + radii.x, to.y + radii.y));
                    }
                }
            }
        }
        any.then_some((min, max))
    }
}

/// Parse a WPF path mini-language string.
pub fn parse_path_data(input: &str) -> XamlResult<PathData> {
    Parser::new(input).parse()
}

struct Parser<'a> {
    input: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            bytes: input.as_bytes(),
            pos: 0,
        }
    }

    fn err(&self, message: impl Into<String>) -> XamlError {
        XamlError::Convert {
            input: self.input.to_string(),
            target: "Geometry",
            message: format!("{} (at byte {})", message.into(), self.pos),
        }
    }

    fn skip_sep(&mut self) {
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b' ' | b'\t' | b'\r' | b'\n' | b',' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn number(&mut self) -> XamlResult<f32> {
        self.skip_sep();
        let start = self.pos;
        if matches!(self.peek(), Some(b'+') | Some(b'-')) {
            self.pos += 1;
        }
        let mut seen_digit = false;
        let mut seen_dot = false;
        while let Some(c) = self.peek() {
            match c {
                b'0'..=b'9' => {
                    seen_digit = true;
                    self.pos += 1;
                }
                b'.' if !seen_dot => {
                    seen_dot = true;
                    self.pos += 1;
                }
                b'e' | b'E' => {
                    self.pos += 1;
                    if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
        if !seen_digit {
            return Err(self.err("expected a number"));
        }
        self.input[start..self.pos]
            .parse()
            .map_err(|e| self.err(format!("bad number: {e}")))
    }

    fn point(&mut self) -> XamlResult<Point> {
        let x = self.number()?;
        let y = self.number()?;
        Ok(Point::new(x, y))
    }

    fn flag(&mut self) -> XamlResult<bool> {
        // WPF accepts 0/1 and also true/false here (SVG only 0/1).
        self.skip_sep();
        match self.peek() {
            Some(b'0') => {
                self.pos += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.pos += 1;
                Ok(true)
            }
            _ => Err(self.err("expected arc flag 0 or 1")),
        }
    }

    fn parse(mut self) -> XamlResult<PathData> {
        let mut data = PathData::default();

        self.skip_sep();
        // Optional fill-rule prefix.
        if matches!(self.peek(), Some(b'F') | Some(b'f')) {
            self.pos += 1;
            self.skip_sep();
            match self.peek() {
                Some(b'0') => {
                    data.fill_rule = FillRule::EvenOdd;
                    self.pos += 1;
                }
                Some(b'1') => {
                    data.fill_rule = FillRule::NonZero;
                    self.pos += 1;
                }
                _ => return Err(self.err("expected 0 or 1 after F")),
            }
        }

        let mut current = Point::new(0.0, 0.0);
        let mut figure: Option<PathFigure> = None;
        // For smooth curve reflection.
        let mut last_cubic_ctrl: Option<Point> = None;
        let mut last_quad_ctrl: Option<Point> = None;

        macro_rules! figure_mut {
            () => {
                match figure.as_mut() {
                    Some(f) => f,
                    None => return Err(self.err("path must start with a move (M/m) command")),
                }
            };
        }

        loop {
            self.skip_sep();
            let Some(cmd) = self.peek() else { break };
            if !cmd.is_ascii_alphabetic() {
                return Err(self.err(format!("expected a command, found `{}`", cmd as char)));
            }
            self.pos += 1;
            let relative = cmd.is_ascii_lowercase();
            let cmd = cmd.to_ascii_uppercase();

            let rel = |p: Point, cur: Point| {
                if relative {
                    Point::new(p.x + cur.x, p.y + cur.y)
                } else {
                    p
                }
            };

            match cmd {
                b'M' => {
                    let p = self.point()?;
                    let p = rel(p, current);
                    if let Some(f) = figure.take() {
                        data.figures.push(f);
                    }
                    figure = Some(PathFigure {
                        start: p,
                        segments: Vec::new(),
                        closed: false,
                    });
                    current = p;
                    last_cubic_ctrl = None;
                    last_quad_ctrl = None;
                    // Additional coordinate pairs after a move are implicit
                    // line-tos.
                    while self.has_number_ahead() {
                        let p = self.point()?;
                        let p = rel(p, current);
                        figure_mut!().segments.push(PathSegment::Line(p));
                        current = p;
                    }
                }
                b'L' => {
                    loop {
                        let p = self.point()?;
                        let p = rel(p, current);
                        figure_mut!().segments.push(PathSegment::Line(p));
                        current = p;
                        if !self.has_number_ahead() {
                            break;
                        }
                    }
                    last_cubic_ctrl = None;
                    last_quad_ctrl = None;
                }
                b'H' | b'V' => {
                    loop {
                        let n = self.number()?;
                        let p = match (cmd, relative) {
                            (b'H', false) => Point::new(n, current.y),
                            (b'H', true) => Point::new(current.x + n, current.y),
                            (b'V', false) => Point::new(current.x, n),
                            (b'V', true) => Point::new(current.x, current.y + n),
                            _ => unreachable!(),
                        };
                        figure_mut!().segments.push(PathSegment::Line(p));
                        current = p;
                        if !self.has_number_ahead() {
                            break;
                        }
                    }
                    last_cubic_ctrl = None;
                    last_quad_ctrl = None;
                }
                b'C' => loop {
                    let c1 = rel(self.point()?, current);
                    let c2 = rel(self.point()?, current);
                    let to = rel(self.point()?, current);
                    figure_mut!().segments.push(PathSegment::Cubic(c1, c2, to));
                    last_cubic_ctrl = Some(c2);
                    last_quad_ctrl = None;
                    current = to;
                    if !self.has_number_ahead() {
                        break;
                    }
                },
                b'S' => loop {
                    let c1 = match last_cubic_ctrl {
                        Some(c) => Point::new(2.0 * current.x - c.x, 2.0 * current.y - c.y),
                        None => current,
                    };
                    let c2 = rel(self.point()?, current);
                    let to = rel(self.point()?, current);
                    figure_mut!().segments.push(PathSegment::Cubic(c1, c2, to));
                    last_cubic_ctrl = Some(c2);
                    last_quad_ctrl = None;
                    current = to;
                    if !self.has_number_ahead() {
                        break;
                    }
                },
                b'Q' => loop {
                    let c = rel(self.point()?, current);
                    let to = rel(self.point()?, current);
                    figure_mut!().segments.push(PathSegment::Quadratic(c, to));
                    last_quad_ctrl = Some(c);
                    last_cubic_ctrl = None;
                    current = to;
                    if !self.has_number_ahead() {
                        break;
                    }
                },
                b'T' => loop {
                    let c = match last_quad_ctrl {
                        Some(c) => Point::new(2.0 * current.x - c.x, 2.0 * current.y - c.y),
                        None => current,
                    };
                    let to = rel(self.point()?, current);
                    figure_mut!().segments.push(PathSegment::Quadratic(c, to));
                    last_quad_ctrl = Some(c);
                    last_cubic_ctrl = None;
                    current = to;
                    if !self.has_number_ahead() {
                        break;
                    }
                },
                b'A' => loop {
                    let radii = self.point()?;
                    let rotation = self.number()?;
                    let large_arc = self.flag()?;
                    let sweep = self.flag()?;
                    let to = rel(self.point()?, current);
                    figure_mut!().segments.push(PathSegment::Arc {
                        radii,
                        rotation,
                        large_arc,
                        sweep,
                        to,
                    });
                    current = to;
                    last_cubic_ctrl = None;
                    last_quad_ctrl = None;
                    if !self.has_number_ahead() {
                        break;
                    }
                },
                b'Z' => {
                    let f = figure_mut!();
                    f.closed = true;
                    current = f.start;
                    let done = figure.take().unwrap();
                    data.figures.push(done);
                    last_cubic_ctrl = None;
                    last_quad_ctrl = None;
                }
                other => {
                    return Err(self.err(format!("unknown command `{}`", other as char)));
                }
            }
        }

        if let Some(f) = figure.take() {
            data.figures.push(f);
        }
        if data.figures.is_empty() {
            return Err(self.err("empty path"));
        }
        Ok(data)
    }

    fn has_number_ahead(&mut self) -> bool {
        self.skip_sep();
        matches!(self.peek(), Some(b'0'..=b'9') | Some(b'+') | Some(b'-') | Some(b'.'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_triangle() {
        let d = parse_path_data("M 0,0 L 10,0 L 5,8 Z").unwrap();
        assert_eq!(d.fill_rule, FillRule::EvenOdd);
        assert_eq!(d.figures.len(), 1);
        let f = &d.figures[0];
        assert_eq!(f.start, Point::new(0.0, 0.0));
        assert_eq!(f.segments.len(), 2);
        assert!(f.closed);
    }

    #[test]
    fn parses_fill_rule_prefix() {
        let d = parse_path_data("F1 M0,0 L1,1").unwrap();
        assert_eq!(d.fill_rule, FillRule::NonZero);
        assert!(!d.figures[0].closed);
    }

    #[test]
    fn parses_relative_commands() {
        let d = parse_path_data("m 10,10 l 5,0 v 5 h -5 z").unwrap();
        let f = &d.figures[0];
        assert_eq!(f.start, Point::new(10.0, 10.0));
        assert_eq!(
            f.segments,
            vec![
                PathSegment::Line(Point::new(15.0, 10.0)),
                PathSegment::Line(Point::new(15.0, 15.0)),
                PathSegment::Line(Point::new(10.0, 15.0)),
            ]
        );
    }

    #[test]
    fn parses_cubic_and_smooth() {
        let d = parse_path_data("M0,0 C 1,1 2,1 3,0 S 5,-1 6,0").unwrap();
        let f = &d.figures[0];
        assert_eq!(f.segments.len(), 2);
        // Smooth control point reflects the previous C's second control (2,1)
        // about the current point (3,0) -> (4,-1).
        match f.segments[1] {
            PathSegment::Cubic(c1, _, to) => {
                assert_eq!(c1, Point::new(4.0, -1.0));
                assert_eq!(to, Point::new(6.0, 0.0));
            }
            _ => panic!("expected cubic"),
        }
    }

    #[test]
    fn parses_quadratic_and_smooth() {
        let d = parse_path_data("M0,0 Q 1,2 2,0 T 4,0").unwrap();
        match d.figures[0].segments[1] {
            PathSegment::Quadratic(c, to) => {
                assert_eq!(c, Point::new(3.0, -2.0));
                assert_eq!(to, Point::new(4.0, 0.0));
            }
            _ => panic!("expected quadratic"),
        }
    }

    #[test]
    fn parses_arcs() {
        let d = parse_path_data("M 10,50 A 20,20 0 1 1 50,90").unwrap();
        match d.figures[0].segments[0] {
            PathSegment::Arc {
                radii,
                large_arc,
                sweep,
                to,
                ..
            } => {
                assert_eq!(radii, Point::new(20.0, 20.0));
                assert!(large_arc);
                assert!(sweep);
                assert_eq!(to, Point::new(50.0, 90.0));
            }
            _ => panic!("expected arc"),
        }
    }

    #[test]
    fn parses_multiple_figures() {
        let d = parse_path_data("M0,0 L1,0 Z M5,5 L6,5").unwrap();
        assert_eq!(d.figures.len(), 2);
        assert!(d.figures[0].closed);
        assert!(!d.figures[1].closed);
    }

    #[test]
    fn implicit_lineto_after_move() {
        let d = parse_path_data("M 0,0 10,0 10,10").unwrap();
        assert_eq!(d.figures[0].segments.len(), 2);
    }

    #[test]
    fn repeated_command_coordinates() {
        let d = parse_path_data("M0,0 L 1,0 2,0 3,0").unwrap();
        assert_eq!(d.figures[0].segments.len(), 3);
    }

    #[test]
    fn negative_and_scientific_numbers() {
        let d = parse_path_data("M-1.5,-2.5 L1e1,2.5e-1").unwrap();
        assert_eq!(d.figures[0].start, Point::new(-1.5, -2.5));
        assert_eq!(
            d.figures[0].segments[0],
            PathSegment::Line(Point::new(10.0, 0.25))
        );
    }

    #[test]
    fn compact_svg_style() {
        // No separators between command and numbers, minus as separator.
        let d = parse_path_data("M0 0L10 0 5 8Z").unwrap();
        assert_eq!(d.figures[0].segments.len(), 2);
    }

    #[test]
    fn control_bounds() {
        let d = parse_path_data("M 0,0 L 10,0 L 5,8 Z").unwrap();
        let (min, max) = d.control_bounds().unwrap();
        assert_eq!(min, Point::new(0.0, 0.0));
        assert_eq!(max, Point::new(10.0, 8.0));
    }

    #[test]
    fn errors() {
        assert!(parse_path_data("").is_err());
        assert!(parse_path_data("L 1,1").is_err()); // no move
        assert!(parse_path_data("M 1").is_err()); // incomplete point
        assert!(parse_path_data("M 0,0 X 1,1").is_err()); // unknown cmd
        assert!(parse_path_data("F2 M0,0").is_err()); // bad fill rule
    }

    #[test]
    fn wpf_doc_example() {
        // From the WPF documentation.
        let d = parse_path_data("M 10,50 C 30,0 70,0 90,50 A 20,20 0 1 1 50,90 Z").unwrap();
        assert_eq!(d.figures.len(), 1);
        assert_eq!(d.figures[0].segments.len(), 2);
        assert!(d.figures[0].closed);
    }
}
