use serde::Serialize;

/// Non-fatal diagnostics. Categories are stable identifiers (snake_case in JSON).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningCategory {
    /// `app://` key that is not `app://<package>:<activity>` with a valid package name.
    MalformedAppKey,
    /// A tag value contains `,`, which Quick Search treats as an alias separator.
    CommaInTagValue,
    /// A tag value is empty or whitespace only.
    BlankTagValue,
    /// A tag for an app in a work/clone profile (`app://pkg:Activity:serial`).
    ProfileAppTag,
    /// A customization record that is not an object or lacks string `key`/`type`/`value`.
    MalformedRecord,
    /// The Kvaesitso archive has no `meta` member.
    MissingSourceMeta,
    /// Kvaesitso `meta.format` is 1.x but not the verified 1.9.
    UntestedSourceFormat,
    /// The source contains no usable app tags at all.
    NoAppTags,
}

impl WarningCategory {
    /// Short human label (e.g. "malformed app key").
    pub fn human(self) -> &'static str {
        match self {
            Self::MalformedAppKey => "malformed app key",
            Self::CommaInTagValue => "comma in tag value",
            Self::BlankTagValue => "blank tag value",
            Self::ProfileAppTag => "work/clone profile app tag",
            Self::MalformedRecord => "malformed customization record",
            Self::MissingSourceMeta => "missing source meta",
            Self::UntestedSourceFormat => "untested source format",
            Self::NoAppTags => "no app tags",
        }
    }
}

/// A warning with only non-sensitive locators: never the tag value itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Warning {
    pub category: WarningCategory,
    pub message: String,
    /// Android package name, when one could be determined.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    /// Numeric `customizations.NNNN` page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u64>,
    /// Index of the record within the page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
}

impl Warning {
    pub(crate) fn new(category: WarningCategory, message: impl Into<String>) -> Self {
        Warning {
            category,
            message: message.into(),
            package: None,
            page: None,
            index: None,
        }
    }

    pub(crate) fn at(mut self, page: u64, index: usize) -> Self {
        self.page = Some(page);
        self.index = Some(index);
        self
    }

    pub(crate) fn package(mut self, package: &str) -> Self {
        self.package = Some(package.to_string());
        self
    }
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(p) = &self.package {
            write!(f, " [package {p}]")?;
        }
        if let (Some(page), Some(i)) = (self.page, self.index) {
            write!(f, " [customizations.{page:04} #{i}]")?;
        }
        Ok(())
    }
}
