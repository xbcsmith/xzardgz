use super::diataxis::DocCategory;
use crate::error::{DocGenError, Result};
use std::path::PathBuf;
use tokio::fs;

pub struct DocumentWriter {
    output_dir: PathBuf,
    overwrite: bool,
}

impl DocumentWriter {
    pub fn new(output_dir: PathBuf, overwrite: bool) -> Self {
        Self {
            output_dir,
            overwrite,
        }
    }

    pub async fn write(
        &self,
        category: DocCategory,
        filename: &str,
        content: &str,
    ) -> Result<PathBuf> {
        let category_dir = self.output_dir.join(category.directory());

        if !category_dir.exists() {
            fs::create_dir_all(&category_dir).await.map_err(|e| {
                DocGenError::Io(format!(
                    "Failed to create directory {:?}: {}",
                    category_dir, e
                ))
            })?;
        }

        let mut path = category_dir.join(filename);
        if path.extension().is_none_or(|ext| ext != "md") {
            path.set_extension("md");
        }

        if path.exists() && !self.overwrite {
            return Err(DocGenError::Io(format!("File already exists: {:?}", path)).into());
        }

        fs::write(&path, content)
            .await
            .map_err(|e| DocGenError::Io(format!("Failed to write file {:?}: {}", path, e)))?;

        Ok(path)
    }
}
