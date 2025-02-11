pub mod progress_bar;

#[derive(Clone)]
pub enum OutputFormat {
    Json,
    Plaintext,
}

impl OutputFormat {
    pub fn is_json(&self) -> bool {
        if let Self::Json = self {
            true
        } else {
            false
        }
    }

    pub fn is_plaintext(&self) -> bool {
        if let Self::Plaintext = self {
            true
        } else {
            false
        }
    }
}


