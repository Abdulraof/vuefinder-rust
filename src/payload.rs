use actix_multipart::form::{tempfile::TempFile, text::Text, MultipartForm};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Query {
    #[serde(default)]
    pub path: String,
}

#[derive(Clone)]
pub struct BoolOrNum(pub bool);

impl<'de> Deserialize<'de> for BoolOrNum {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum BoolOrNumInner {
            Bool(bool),
            Str(String),
        }

        let inner = BoolOrNumInner::deserialize(deserializer)?;
        match inner {
            BoolOrNumInner::Bool(b) => Ok(BoolOrNum(b)),
            BoolOrNumInner::Str(s) => {
                if s == "1" {
                    Ok(BoolOrNum(true))
                } else if s == "0" {
                    Ok(BoolOrNum(false))
                } else if s.to_lowercase() == "true" {
                    Ok(BoolOrNum(true))
                } else if s.to_lowercase() == "false" {
                    Ok(BoolOrNum(false))
                } else {
                    Err(serde::de::Error::custom("invalid boolean value"))
                }
            }
        }
    }
}

impl Default for BoolOrNum {
    fn default() -> Self {
        BoolOrNum(false)
    }
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub path: String,
    pub filter: Option<String>,
    #[serde(default, deserialize_with = "deserialize_bool_or_num")]
    pub deep: Option<BoolOrNum>,
    pub size: Option<String>,
}

fn deserialize_bool_or_num<'de, D>(deserializer: D) -> Result<Option<BoolOrNum>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<BoolOrNum>::deserialize(deserializer)
}

#[derive(Deserialize)]
pub struct NewFolderRequest {
    pub name: String,
    pub path: String,
}

#[derive(Deserialize)]
pub struct NewFileRequest {
    pub name: String,
    pub path: String,
}

#[derive(Deserialize)]
pub struct RenameRequest {
    pub path: String,
    pub item: String,
    pub name: String,
}

#[derive(Deserialize)]
pub struct MoveRequest {
    pub path: String,
    #[serde(default)]
    pub items: Vec<FileItem>,
    #[serde(default)]
    pub sources: Vec<String>,
    pub destination: String,
}

impl MoveRequest {
    pub fn resolve_items(&self) -> Vec<FileItem> {
        if !self.items.is_empty() {
            return self.items.clone();
        }
        self.sources
            .iter()
            .map(|s| FileItem {
                path: s.clone(),
                r#type: "file".to_string(),
            })
            .collect()
    }
}

#[derive(Deserialize)]
pub struct CopyRequest {
    pub path: String,
    #[serde(default)]
    pub items: Vec<FileItem>,
    #[serde(default)]
    pub sources: Vec<String>,
    pub destination: String,
}

impl CopyRequest {
    pub fn resolve_items(&self) -> Vec<FileItem> {
        if !self.items.is_empty() {
            return self.items.clone();
        }
        self.sources
            .iter()
            .map(|s| FileItem {
                path: s.clone(),
                r#type: "file".to_string(),
            })
            .collect()
    }
}

#[derive(Deserialize)]
pub struct DeleteRequest {
    pub path: String,
    pub items: Vec<FileItem>,
}

#[derive(Deserialize)]
pub struct ArchiveRequest {
    pub name: String,
    pub items: Vec<FileItem>,
    pub path: String,
}

#[derive(Deserialize)]
pub struct UnarchiveRequest {
    pub item: String,
    pub path: String,
}

#[derive(Deserialize)]
pub struct SaveRequest {
    pub path: String,
    pub content: String,
}

#[derive(Deserialize, Clone)]
pub struct FileItem {
    pub path: String,
    pub r#type: String,
}

#[derive(Debug, MultipartForm)]
pub struct UploadForm {
    #[multipart(rename = "path")]
    pub path: Text<String>,

    #[multipart(rename = "name")]
    pub name: Text<String>,

    #[multipart(rename = "file")]
    pub file: TempFile,
}
