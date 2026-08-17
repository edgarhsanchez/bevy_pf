//! Selector-based styling: `<Style Selector="Button.danger:pointerover">`.
//!
//! A third styling dialect beside WPF's `Style`/`TargetType` and MAUI's. It
//! looks like CSS, but it deliberately is NOT CSS in the one place that
//! matters most: **no specificity is computed**. Which style wins is
//! decided by two things only —
//!
//! 1. a bucket: did the selector produce an *activator* (any class,
//!    pseudo-class or `[Prop=Val]` part — length and shape are irrelevant),
//!    and did it come from a ControlTheme or a `Styles` collection;
//! 2. within a bucket, **last attached wins**.
//!
//! That is the same rule `PfPropertyStore` already implements: highest tier
//! wins, and the last write to a tier is that tier's value. So selectors need
//! no priority arithmetic here either — bucket picks the tier, attach order
//! does the rest.
//!
//! Selectors are built LEFT TO RIGHT, each fragment wrapping the one before
//! it. `Button > TextBlock` is therefore
//! "a TextBlock whose parent is a Button", with the TextBlock outermost:
//! matching starts at the element and walks back up the chain.

use std::fmt;

/// One parsed selector.
#[derive(Debug, Clone, PartialEq)]
pub enum Sel {
    /// Matches the element itself — the base every chain is built on, and
    /// what a bare `^` resolves to inside a nested style.
    Any,
    /// `A, B` — matches if any branch matches.
    Or(Vec<Sel>),
    /// `Button` — the element's own type name.
    OfType { prev: Box<Sel>, name: String },
    /// `:is(Button)` — the type or anything deriving from it. bevy_pf has no
    /// inheritance among element kinds, so this behaves as `OfType` until
    /// there is a hierarchy to consult.
    Is { prev: Box<Sel>, name: String },
    /// `.danger`, and pseudo-classes such as `:pointerover`, which are
    /// modelled as classes whose name begins with `:`.
    Class { prev: Box<Sel>, name: String },
    /// `#PART_ContentPresenter`
    Name { prev: Box<Sel>, name: String },
    /// `[IsVisible=True]` / `[(Grid.Row)=1]`
    Property {
        prev: Box<Sel>,
        owner: Option<String>,
        name: String,
        value: String,
    },
    /// `:nth-child(2n+1)` / `:nth-last-child(...)`
    NthChild {
        prev: Box<Sel>,
        step: i32,
        offset: i32,
        from_end: bool,
    },
    /// `:not(...)`
    Not { prev: Box<Sel>, inner: Box<Sel> },
    /// `^` — the enclosing style's selector, in a nested `<Style>`.
    Nesting(Box<Sel>),
    /// `>` — the boxed selector must match the element's PARENT.
    Child(Box<Sel>),
    /// a space — the boxed selector must match some ancestor.
    Descendant(Box<Sel>),
    /// `/template/` — the boxed selector must match the templated parent.
    Template(Box<Sel>),
}

impl Sel {
    /// Does this selector depend on state that can change while the app runs?
    ///
    /// This is the "has an activator" test, and it decides the bucket — not
    /// how specific the selector looks. A class, a pseudo-class or a property
    /// test makes membership dynamic; a type, a name or a structural
    /// relationship is fixed once the tree exists.
    pub fn has_activator(&self) -> bool {
        match self {
            Sel::Any => false,
            Sel::Or(branches) => branches.iter().any(Sel::has_activator),
            Sel::Class { .. } | Sel::Property { .. } => true,
            // `:not(.x)` is dynamic exactly when its inner selector is.
            Sel::Not { prev, inner } => prev.has_activator() || inner.has_activator(),
            Sel::OfType { prev, .. }
            | Sel::Is { prev, .. }
            | Sel::Name { prev, .. }
            | Sel::NthChild { prev, .. } => prev.has_activator(),
            Sel::Nesting(prev) | Sel::Child(prev) | Sel::Descendant(prev) | Sel::Template(prev) => {
                prev.has_activator()
            }
        }
    }

    /// Resolve `^` against the enclosing style's selector.
    ///
    /// `<Style Selector="Button">` containing `<Style Selector="^:pointerover">`
    /// means "that same Button, while the pointer is over it". Substitution
    /// happens once when the nested style is collected, so matching never has
    /// to know it was nested.
    pub fn substitute_nesting(&self, parent: &Sel) -> Sel {
        let sub = |s: &Sel| Box::new(s.substitute_nesting(parent));
        match self {
            // The `^` itself becomes the parent selector.
            Sel::Nesting(prev) => match &**prev {
                Sel::Any => parent.clone(),
                other => other.substitute_nesting(parent),
            },
            Sel::Any => Sel::Any,
            Sel::Or(branches) => Sel::Or(
                branches
                    .iter()
                    .map(|b| b.substitute_nesting(parent))
                    .collect(),
            ),
            Sel::OfType { prev, name } => Sel::OfType {
                prev: sub(prev),
                name: name.clone(),
            },
            Sel::Is { prev, name } => Sel::Is {
                prev: sub(prev),
                name: name.clone(),
            },
            Sel::Class { prev, name } => Sel::Class {
                prev: sub(prev),
                name: name.clone(),
            },
            Sel::Name { prev, name } => Sel::Name {
                prev: sub(prev),
                name: name.clone(),
            },
            Sel::Property {
                prev,
                owner,
                name,
                value,
            } => Sel::Property {
                prev: sub(prev),
                owner: owner.clone(),
                name: name.clone(),
                value: value.clone(),
            },
            Sel::NthChild {
                prev,
                step,
                offset,
                from_end,
            } => Sel::NthChild {
                prev: sub(prev),
                step: *step,
                offset: *offset,
                from_end: *from_end,
            },
            Sel::Not { prev, inner } => Sel::Not {
                prev: sub(prev),
                inner: Box::new(inner.substitute_nesting(parent)),
            },
            Sel::Child(prev) => Sel::Child(sub(prev)),
            Sel::Descendant(prev) => Sel::Descendant(sub(prev)),
            Sel::Template(prev) => Sel::Template(sub(prev)),
        }
    }

    /// Every class and pseudo-class this selector tests, so the matcher can
    /// compile them into trigger conditions.
    pub fn classes(&self, out: &mut Vec<String>) {
        match self {
            Sel::Any => {}
            Sel::Or(branches) => branches.iter().for_each(|b| b.classes(out)),
            Sel::Class { prev, name } => {
                out.push(name.clone());
                prev.classes(out);
            }
            Sel::Not { prev, inner } => {
                prev.classes(out);
                inner.classes(out);
            }
            Sel::OfType { prev, .. }
            | Sel::Is { prev, .. }
            | Sel::Name { prev, .. }
            | Sel::Property { prev, .. }
            | Sel::NthChild { prev, .. } => prev.classes(out),
            Sel::Nesting(prev) | Sel::Child(prev) | Sel::Descendant(prev) | Sel::Template(prev) => {
                prev.classes(out)
            }
        }
    }
}

/// What the matcher knows about one element in the chain being built.
#[derive(Debug, Clone, Default)]
pub struct ElementInfo {
    /// The XAML element name, e.g. `Button`.
    pub type_name: String,
    /// `x:Name` / `Name`.
    pub name: Option<String>,
    /// Style classes, and pseudo-classes as `:pointerover`-style entries.
    pub classes: Vec<String>,
    /// 0-based position among its siblings, and how many there are — for
    /// `:nth-child`.
    pub child_index: usize,
    pub sibling_count: usize,
}

/// The style classes an element declares: `Classes="h1 accent"`, split on
/// whitespace, like any style-class list.
pub fn classes_of(node: &bevy_pf_xaml::XamlNode) -> Vec<String> {
    match node.attribute("Classes") {
        Some(bevy_pf_xaml::XamlValue::Str(s)) => s.split_whitespace().map(str::to_string).collect(),
        _ => Vec::new(),
    }
}

/// Does the STRUCTURAL part of `sel` match, ignoring anything that can
/// change while the app runs?
///
/// Classes and pseudo-classes are deliberately treated as satisfied here:
/// they are re-checked every frame by the trigger runtime, so an element
/// that could *ever* match is attached now and activates later. Type, name
/// and structure cannot change once the tree exists, so if they miss, the
/// element can never match and nothing is attached at all.
pub fn matches_static(sel: &Sel, stack: &[ElementInfo]) -> bool {
    match sel {
        Sel::Class { prev, .. } => matches_static(prev, stack),
        Sel::Or(branches) => branches.iter().any(|b| matches_static(b, stack)),
        Sel::Not { prev, inner } => {
            // A `:not()` over a dynamic inner selector cannot be decided
            // now; over a static one it is final.
            matches_static(prev, stack) && (inner.has_activator() || !matches(inner, stack))
        }
        Sel::OfType { prev, name } | Sel::Is { prev, name } => {
            stack.last().is_some_and(|e| e.type_name == *name) && matches_static(prev, stack)
        }
        Sel::Name { prev, name } => {
            stack
                .last()
                .is_some_and(|e| e.name.as_deref() == Some(name.as_str()))
                && matches_static(prev, stack)
        }
        Sel::Child(prev) => stack.len() >= 2 && matches_static(prev, &stack[..stack.len() - 1]),
        Sel::Descendant(prev) => (1..stack.len())
            .rev()
            .any(|k| matches_static(prev, &stack[..k])),
        // Everything else keeps the strict answer.
        _ => matches(sel, stack),
    }
}

/// Does `sel` match the LAST element of `stack`?
///
/// `stack` runs root-first, with the element under test last, so the
/// combinators walk backwards through it: `Child` steps one entry left,
/// `Descendant` tries every prefix.
pub fn matches(sel: &Sel, stack: &[ElementInfo]) -> bool {
    let Some(element) = stack.last() else {
        return false;
    };
    match sel {
        Sel::Any => true,
        Sel::Or(branches) => branches.iter().any(|b| matches(b, stack)),
        Sel::OfType { prev, name } | Sel::Is { prev, name } => {
            element.type_name == *name && matches(prev, stack)
        }
        Sel::Class { prev, name } => {
            element.classes.iter().any(|c| c == name) && matches(prev, stack)
        }
        Sel::Name { prev, name } => {
            element.name.as_deref() == Some(name.as_str()) && matches(prev, stack)
        }
        Sel::NthChild {
            prev,
            step,
            offset,
            from_end,
        } => {
            let position = if *from_end {
                element.sibling_count.saturating_sub(element.child_index)
            } else {
                element.child_index + 1
            } as i32;
            let hit = if *step == 0 {
                position == *offset
            } else {
                // position = step*n + offset for some whole n >= 0
                let delta = position - *offset;
                delta % *step == 0 && delta / *step >= 0
            };
            hit && matches(prev, stack)
        }
        Sel::Not { prev, inner } => matches(prev, stack) && !matches(inner, stack),
        Sel::Child(prev) => stack.len() >= 2 && matches(prev, &stack[..stack.len() - 1]),
        Sel::Descendant(prev) => (1..stack.len()).rev().any(|k| matches(prev, &stack[..k])),
        // Phase 3 / phase 5: a templated parent and a live property are not
        // available at this point, so these do not match rather than
        // pretending to.
        Sel::Template(_) | Sel::Nesting(_) | Sel::Property { .. } => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorError(pub String);

impl fmt::Display for SelectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Parse a `Selector=` string.
pub fn parse(input: &str) -> Result<Sel, SelectorError> {
    let mut p = Parser {
        chars: input.char_indices().peekable(),
        input,
    };
    let sel = p.parse_or()?;
    p.skip_ws();
    if let Some(&(i, c)) = p.chars.peek() {
        return Err(SelectorError(format!(
            "unexpected `{c}` at byte {i} in selector `{input}`"
        )));
    }
    Ok(sel)
}

struct Parser<'a> {
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    input: &'a str,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while let Some(&(_, c)) = self.chars.peek() {
            if c.is_whitespace() {
                self.chars.next();
            } else {
                break;
            }
        }
    }

    /// Was there whitespace here? Descendant combinators are spelled with it,
    /// so it cannot simply be skipped.
    fn eat_ws(&mut self) -> bool {
        let mut seen = false;
        while let Some(&(_, c)) = self.chars.peek() {
            if c.is_whitespace() {
                seen = true;
                self.chars.next();
            } else {
                break;
            }
        }
        seen
    }

    fn parse_or(&mut self) -> Result<Sel, SelectorError> {
        let mut branches = vec![self.parse_chain()?];
        loop {
            self.skip_ws();
            match self.chars.peek() {
                Some(&(_, ',')) => {
                    self.chars.next();
                    self.skip_ws();
                    branches.push(self.parse_chain()?);
                }
                _ => break,
            }
        }
        Ok(if branches.len() == 1 {
            branches.pop().expect("just checked")
        } else {
            Sel::Or(branches)
        })
    }

    /// One comma-free chain: fragments joined by ` `, `>` or `/template/`.
    fn parse_chain(&mut self) -> Result<Sel, SelectorError> {
        self.skip_ws();
        let mut sel = self.parse_fragment(Sel::Any)?;
        loop {
            let had_ws = self.eat_ws();
            match self.chars.peek().copied() {
                Some((_, '>')) => {
                    self.chars.next();
                    self.skip_ws();
                    sel = self.parse_fragment(Sel::Child(Box::new(sel)))?;
                }
                Some((_, '/')) => {
                    self.expect_word("/template/")?;
                    self.skip_ws();
                    sel = self.parse_fragment(Sel::Template(Box::new(sel)))?;
                }
                // Whitespace before another fragment is a descendant combinator;
                // whitespace before `,` or the end is just trailing.
                Some((_, c)) if had_ws && c != ',' => {
                    sel = self.parse_fragment(Sel::Descendant(Box::new(sel)))?;
                }
                _ => break,
            }
        }
        Ok(sel)
    }

    fn expect_word(&mut self, word: &str) -> Result<(), SelectorError> {
        for expected in word.chars() {
            match self.chars.next() {
                Some((_, c)) if c == expected => {}
                _ => {
                    return Err(SelectorError(format!(
                        "expected `{word}` in selector `{}`",
                        self.input
                    )));
                }
            }
        }
        Ok(())
    }

    /// A run of `Type`, `.class`, `#name`, `:pseudo`, `[prop=val]`, `^`
    /// with no separator between them.
    fn parse_fragment(&mut self, base: Sel) -> Result<Sel, SelectorError> {
        let mut sel = base;
        let mut any = false;
        loop {
            match self.chars.peek().copied() {
                Some((_, '.')) => {
                    self.chars.next();
                    let name = self.ident("a class name")?;
                    sel = Sel::Class {
                        prev: Box::new(sel),
                        name,
                    };
                }
                Some((_, '#')) => {
                    self.chars.next();
                    let name = self.ident("a name")?;
                    sel = Sel::Name {
                        prev: Box::new(sel),
                        name,
                    };
                }
                Some((_, '^')) => {
                    self.chars.next();
                    sel = Sel::Nesting(Box::new(sel));
                }
                Some((_, ':')) => {
                    self.chars.next();
                    sel = self.parse_colon(sel)?;
                }
                Some((_, '[')) => {
                    self.chars.next();
                    sel = self.parse_property(sel)?;
                }
                Some((_, c)) if c.is_alphanumeric() || c == '_' || c == '|' => {
                    let name = self.ident("a type name")?;
                    sel = Sel::OfType {
                        prev: Box::new(sel),
                        name,
                    };
                }
                _ => break,
            }
            any = true;
        }
        if !any {
            return Err(SelectorError(format!(
                "empty selector fragment in `{}`",
                self.input
            )));
        }
        Ok(sel)
    }

    fn parse_colon(&mut self, sel: Sel) -> Result<Sel, SelectorError> {
        let word = self.ident("a pseudo-class")?;
        match word.as_str() {
            "is" | "not" => {
                self.expect_word("(")?;
                let inner = self.parse_or()?;
                self.skip_ws();
                self.expect_word(")")?;
                if word == "not" {
                    Ok(Sel::Not {
                        prev: Box::new(sel),
                        inner: Box::new(inner),
                    })
                } else {
                    // `:is(Type)` narrows to a type; anything else is not
                    // something bevy_pf can answer.
                    match inner {
                        Sel::OfType { name, .. } => Ok(Sel::Is {
                            prev: Box::new(sel),
                            name,
                        }),
                        _ => Err(SelectorError(format!(
                            ":is() takes a type name in `{}`",
                            self.input
                        ))),
                    }
                }
            }
            "nth-child" | "nth-last-child" => {
                self.expect_word("(")?;
                let (step, offset) = self.parse_nth()?;
                self.expect_word(")")?;
                Ok(Sel::NthChild {
                    prev: Box::new(sel),
                    step,
                    offset,
                    from_end: word == "nth-last-child",
                })
            }
            // Every other pseudo-class is a class whose name starts with ':',
            // which is how the dialect models them.
            other => Ok(Sel::Class {
                prev: Box::new(sel),
                name: format!(":{other}"),
            }),
        }
    }

    /// `2n+1`, `odd`, `even`, `3`, `-n+2`.
    fn parse_nth(&mut self) -> Result<(i32, i32), SelectorError> {
        self.skip_ws();
        let mut raw = String::new();
        while let Some(&(_, c)) = self.chars.peek() {
            if c == ')' {
                break;
            }
            raw.push(c);
            self.chars.next();
        }
        let raw = raw.trim().to_ascii_lowercase();
        match raw.as_str() {
            "odd" => return Ok((2, 1)),
            "even" => return Ok((2, 0)),
            _ => {}
        }
        let bad = || SelectorError(format!("bad nth-child `{raw}` in `{}`", self.input));
        let Some(n) = raw.find('n') else {
            return Ok((0, raw.parse::<i32>().map_err(|_| bad())?));
        };
        let (step_src, rest) = raw.split_at(n);
        let step = match step_src.trim() {
            "" | "+" => 1,
            "-" => -1,
            s => s.parse::<i32>().map_err(|_| bad())?,
        };
        let rest = rest[1..].trim().replace(' ', "");
        let offset = if rest.is_empty() {
            0
        } else {
            rest.parse::<i32>().map_err(|_| bad())?
        };
        Ok((step, offset))
    }

    fn parse_property(&mut self, sel: Sel) -> Result<Sel, SelectorError> {
        self.skip_ws();
        // `[(Owner.Prop)=value]` names an attached property.
        let parenthesised = matches!(self.chars.peek(), Some(&(_, '(')));
        if parenthesised {
            self.chars.next();
        }
        let mut name = String::new();
        while let Some(&(_, c)) = self.chars.peek() {
            if c == '=' || c == ')' {
                break;
            }
            name.push(c);
            self.chars.next();
        }
        if parenthesised {
            self.expect_word(")")?;
        }
        self.expect_word("=")?;
        let mut value = String::new();
        while let Some(&(_, c)) = self.chars.peek() {
            if c == ']' {
                break;
            }
            value.push(c);
            self.chars.next();
        }
        self.expect_word("]")?;
        let name = name.trim().to_string();
        let (owner, name) = match name.split_once('.') {
            Some((o, p)) => (Some(o.to_string()), p.to_string()),
            None => (None, name),
        };
        Ok(Sel::Property {
            prev: Box::new(sel),
            owner,
            name,
            value: value.trim().to_string(),
        })
    }

    fn ident(&mut self, what: &str) -> Result<String, SelectorError> {
        let mut out = String::new();
        while let Some(&(_, c)) = self.chars.peek() {
            if c.is_alphanumeric() || c == '_' || c == '-' || c == '|' {
                out.push(c);
                self.chars.next();
            } else {
                break;
            }
        }
        if out.is_empty() {
            return Err(SelectorError(format!(
                "expected {what} in selector `{}`",
                self.input
            )));
        }
        // `local|Button` — the xmlns prefix is not something bevy_pf resolves;
        // the local name is what matches an element kind.
        Ok(out.rsplit('|').next().unwrap_or(&out).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of_type(name: &str) -> Sel {
        Sel::OfType {
            prev: Box::new(Sel::Any),
            name: name.into(),
        }
    }

    #[test]
    fn a_bare_type_is_the_common_case() {
        assert_eq!(parse("Button").unwrap(), of_type("Button"));
    }

    #[test]
    fn classes_names_and_pseudo_classes_chain_onto_a_type() {
        let sel = parse("Button.danger#go:pointerover").unwrap();
        // Built left to right, so the LAST fragment is outermost.
        let Sel::Class { name, prev } = &sel else {
            panic!("expected the pseudo-class outermost, got {sel:?}");
        };
        assert_eq!(name, ":pointerover", "a pseudo-class is a class named `:x`");
        let Sel::Name { name, .. } = &**prev else {
            panic!("expected #go beneath it");
        };
        assert_eq!(name, "go");
    }

    #[test]
    fn combinators_wrap_what_came_before() {
        // "a TextBlock whose parent is a Button"
        let sel = parse("Button > TextBlock").unwrap();
        let Sel::OfType { name, prev } = &sel else {
            panic!("outermost should be the TextBlock, got {sel:?}");
        };
        assert_eq!(name, "TextBlock");
        assert!(
            matches!(&**prev, Sel::Child(_)),
            "joined by a child combinator"
        );

        let sel = parse("Button TextBlock").unwrap();
        let Sel::OfType { prev, .. } = &sel else {
            panic!("got {sel:?}");
        };
        assert!(
            matches!(&**prev, Sel::Descendant(_)),
            "space is a descendant"
        );
    }

    #[test]
    fn whitespace_around_a_comma_is_not_a_descendant() {
        // The trap in hand-written parsing: ` , ` must not read as a
        // descendant combinator followed by a broken fragment.
        let sel = parse("Button.toolBar, ToggleButton.toolBar").unwrap();
        let Sel::Or(branches) = sel else {
            panic!("expected two branches");
        };
        assert_eq!(branches.len(), 2);
    }

    #[test]
    fn template_and_nesting_parse() {
        let sel = parse("CheckBox:pointerover /template/ Grid#RootGrid").unwrap();
        let mut classes = Vec::new();
        sel.classes(&mut classes);
        assert!(classes.contains(&":pointerover".to_string()));

        let sel = parse("^:disabled /template/ ContentPresenter#PART_ContentPresenter").unwrap();
        assert!(sel.has_activator(), ":disabled makes it activated");
    }

    #[test]
    fn nth_child_accepts_every_spelling() {
        let cases = [
            ("Button:nth-child(1)", (0, 1, false)),
            ("Button:nth-child(2n)", (2, 0, false)),
            ("Button:nth-child(2n+1)", (2, 1, false)),
            ("Button:nth-child(-n+2)", (-1, 2, false)),
            ("Button:nth-child(odd)", (2, 1, false)),
            ("Button:nth-child(even)", (2, 0, false)),
            ("Button:nth-last-child(1)", (0, 1, true)),
        ];
        for (src, (step, offset, from_end)) in cases {
            let sel = parse(src).unwrap_or_else(|e| panic!("{src}: {e}"));
            let Sel::NthChild {
                step: s,
                offset: o,
                from_end: f,
                ..
            } = sel
            else {
                panic!("{src} did not parse to nth-child: {sel:?}");
            };
            assert_eq!((s, o, f), (step, offset, from_end), "{src}");
        }
    }

    #[test]
    fn property_selectors_parse_plain_and_attached() {
        let sel = parse("Button[IsVisible=True]").unwrap();
        let Sel::Property {
            owner, name, value, ..
        } = &sel
        else {
            panic!("got {sel:?}");
        };
        assert_eq!(
            (owner.as_deref(), name.as_str(), value.as_str()),
            (None, "IsVisible", "True")
        );

        let sel = parse("Button[(Grid.Row)=1]").unwrap();
        let Sel::Property { owner, name, .. } = &sel else {
            panic!("got {sel:?}");
        };
        assert_eq!(owner.as_deref(), Some("Grid"));
        assert_eq!(name, "Row");
    }

    #[test]
    fn is_and_not_parse() {
        assert!(matches!(parse(":is(Button)").unwrap(), Sel::Is { .. }));
        assert!(matches!(
            parse("Button:not(.danger)").unwrap(),
            Sel::Not { .. }
        ));
    }

    #[test]
    fn the_activator_bucket_is_about_state_not_length() {
        // The rule that decides precedence: a long structural selector is
        // NOT activated, and a one-character class IS.
        assert!(
            !parse("Window > Grid > StackPanel Button")
                .unwrap()
                .has_activator()
        );
        assert!(!parse("Button#go").unwrap().has_activator());
        assert!(!parse("Button:nth-child(2)").unwrap().has_activator());
        assert!(parse(".x").unwrap().has_activator());
        assert!(parse("Button:pointerover").unwrap().has_activator());
        assert!(parse("Button[IsVisible=True]").unwrap().has_activator());
        assert!(
            parse("Button, .x").unwrap().has_activator(),
            "a branch with an activator activates the whole selector"
        );
    }

    #[test]
    fn a_namespace_prefix_is_reduced_to_the_local_name() {
        assert_eq!(parse("local|Gauge").unwrap(), of_type("Gauge"));
    }

    #[test]
    fn malformed_selectors_are_errors_not_silent_mismatches() {
        // A selector that silently matches nothing is the worst outcome: the
        // style just never applies and nothing says why.
        for bad in ["", ".", "#", "Button >", "Button:nth-child(q)", "Button["] {
            assert!(parse(bad).is_err(), "`{bad}` should be rejected");
        }
    }
}

#[cfg(test)]
mod match_tests {
    use super::*;

    fn el(type_name: &str) -> ElementInfo {
        ElementInfo {
            type_name: type_name.into(),
            sibling_count: 1,
            ..Default::default()
        }
    }

    fn named(type_name: &str, name: &str) -> ElementInfo {
        ElementInfo {
            name: Some(name.into()),
            ..el(type_name)
        }
    }

    fn classed(type_name: &str, classes: &[&str]) -> ElementInfo {
        ElementInfo {
            classes: classes.iter().map(|c| c.to_string()).collect(),
            ..el(type_name)
        }
    }

    fn nth(type_name: &str, index: usize, count: usize) -> ElementInfo {
        ElementInfo {
            child_index: index,
            sibling_count: count,
            ..el(type_name)
        }
    }

    fn hits(selector: &str, stack: &[ElementInfo]) -> bool {
        matches(&parse(selector).expect("selector parses"), stack)
    }

    #[test]
    fn a_type_selector_matches_only_that_type() {
        assert!(hits("Button", &[el("Button")]));
        assert!(!hits("Button", &[el("TextBlock")]));
    }

    #[test]
    fn a_child_combinator_looks_exactly_one_level_up() {
        let direct = [el("UniformGrid"), el("Button")];
        let nested = [el("UniformGrid"), el("StackPanel"), el("Button")];
        assert!(hits("UniformGrid > Button", &direct));
        assert!(
            !hits("UniformGrid > Button", &nested),
            "a grandchild is not a child"
        );
    }

    #[test]
    fn a_descendant_combinator_looks_all_the_way_up() {
        let nested = [el("Window"), el("StackPanel"), el("Border"), el("Button")];
        assert!(hits("Window Button", &nested));
        assert!(hits("StackPanel Button", &nested));
        assert!(!hits("ListBox Button", &nested));
    }

    #[test]
    fn chains_must_match_in_order() {
        let stack = [el("Window"), el("Border"), el("Button")];
        assert!(hits("Window Border > Button", &stack));
        assert!(
            !hits("Border Window > Button", &stack),
            "the ancestor order is part of the selector"
        );
    }

    #[test]
    fn names_and_classes_narrow_a_type() {
        assert!(hits("Button#go", &[named("Button", "go")]));
        assert!(!hits("Button#go", &[named("Button", "stop")]));
        assert!(hits("TextBlock.h1", &[classed("TextBlock", &["h1"])]));
        assert!(!hits("TextBlock.h1", &[classed("TextBlock", &["h2"])]));
        assert!(
            hits(".h1", &[classed("TextBlock", &["a", "h1"])]),
            "one of several classes is enough"
        );
    }

    #[test]
    fn comma_is_alternation() {
        assert!(hits(
            "Button.toolBar, ToggleButton.toolBar",
            &[classed("ToggleButton", &["toolBar"])]
        ));
        assert!(!hits(
            "Button.toolBar, ToggleButton.toolBar",
            &[classed("CheckBox", &["toolBar"])]
        ));
    }

    #[test]
    fn nth_child_counts_from_one_like_css() {
        assert!(hits("Button:nth-child(1)", &[nth("Button", 0, 3)]));
        assert!(!hits("Button:nth-child(1)", &[nth("Button", 1, 3)]));
        // odd = 1st, 3rd, ...
        assert!(hits("Button:nth-child(odd)", &[nth("Button", 0, 4)]));
        assert!(!hits("Button:nth-child(odd)", &[nth("Button", 1, 4)]));
        assert!(hits("Button:nth-child(even)", &[nth("Button", 1, 4)]));
        // -n+2 selects the first two
        assert!(hits("Button:nth-child(-n+2)", &[nth("Button", 0, 5)]));
        assert!(hits("Button:nth-child(-n+2)", &[nth("Button", 1, 5)]));
        assert!(!hits("Button:nth-child(-n+2)", &[nth("Button", 2, 5)]));
        // counted from the end
        assert!(hits("Button:nth-last-child(1)", &[nth("Button", 3, 4)]));
        assert!(!hits("Button:nth-last-child(1)", &[nth("Button", 2, 4)]));
    }

    #[test]
    fn not_inverts_only_its_inner_selector() {
        assert!(hits("Button:not(.danger)", &[classed("Button", &["safe"])]));
        assert!(!hits(
            "Button:not(.danger)",
            &[classed("Button", &["danger"])]
        ));
        assert!(
            !hits("Button:not(.danger)", &[classed("TextBlock", &["safe"])]),
            "the outer type must still match"
        );
    }

    #[test]
    fn a_selector_needing_state_we_do_not_have_yet_does_not_match() {
        // Template and property selectors land in later phases. Not matching
        // is the honest answer; matching would style the wrong thing.
        assert!(!hits(
            "Button /template/ ContentPresenter",
            &[el("Button"), el("ContentPresenter")]
        ));
        assert!(!hits("Button[IsVisible=True]", &[el("Button")]));
    }
}

/// Compile one selector class into a runtime condition.
///
/// Pseudo-classes are modelled as classes whose name begins with `:`, and
/// each of the built-in ones is a state bevy_pf's trigger runtime already
/// tracks for WPF — which is why selector styling needs no new runtime at
/// all. A `:pseudo` this crate does not know falls back to a plain class
/// test, so a control that sets it as a real class still works.
pub fn pseudo_class_condition(name: &str) -> crate::triggers::ResolvedCondition {
    use crate::triggers::ResolvedCondition as C;
    match name {
        ":pointerover" | ":hover" => C::MouseOver(true),
        ":pressed" => C::Pressed(true),
        ":checked" => C::Checked(true),
        ":unchecked" => C::Checked(false),
        ":disabled" => C::Enabled(false),
        ":enabled" => C::Enabled(true),
        ":focus" | ":focus-within" | ":focused" => C::Focused(true),
        ":selected" => C::Selected(true),
        other => C::HasClass(other.to_string()),
    }
}
