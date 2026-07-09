use bevy_pf_xaml::XamlError;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PfError {
    #[error(transparent)]
    Xaml(#[from] XamlError),

    #[error("resource error: {0}")]
    Resource(String),

    #[error("instantiation error: {0}")]
    Instantiate(String),
}

impl PfError {
    pub fn resource(msg: impl Into<String>) -> Self {
        Self::Resource(msg.into())
    }

    pub fn instantiate(msg: impl Into<String>) -> Self {
        Self::Instantiate(msg.into())
    }
}
