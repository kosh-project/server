#[derive(PartialEq, Eq, Default)]
pub enum Selected {
    Module,
    Level,
    #[default]
    Pattern,
}

impl Selected {
    pub(crate) const fn next(&self) -> Self {
        match self {
            Self::Level => Self::Pattern,
            Self::Module => Self::Level,
            Self::Pattern => Self::Module,
        }
    }

    pub(crate) const fn prev(&self) -> Self {
        match self {
            Self::Level => Self::Module,
            Self::Module => Self::Pattern,
            Self::Pattern => Self::Level,
        }
    }

    pub(crate) const fn is_pattern(&self) -> bool {
        matches!(self, Self::Pattern)
    }

    pub(crate) const fn is_module(&self) -> bool {
        matches!(self, Self::Module)
    }

    pub(crate) const fn is_level(&self) -> bool {
        matches!(self, Self::Level)
    }
}
