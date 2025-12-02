use crate::error::RepositoryError;
use git2::{Repository, StatusOptions};
use std::path::Path;

pub struct GitRepository {
    repo: Repository,
}

impl GitRepository {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, RepositoryError> {
        let repo = Repository::open(path).map_err(|e| RepositoryError::Git(e.to_string()))?;
        Ok(Self { repo })
    }

    pub fn get_status(&self) -> Result<String, RepositoryError> {
        let mut opts = StatusOptions::new();
        opts.include_untracked(true);

        let statuses = self
            .repo
            .statuses(Some(&mut opts))
            .map_err(|e| RepositoryError::Git(e.to_string()))?;

        let mut output = String::new();
        for entry in statuses.iter() {
            let path = entry.path().unwrap_or("unknown");
            let status = entry.status();
            output.push_str(&format!("{:?} {}\n", status, path));
        }

        if output.is_empty() {
            Ok("Clean".to_string())
        } else {
            Ok(output)
        }
    }

    pub fn get_diff(&self) -> Result<String, RepositoryError> {
        // Diff index to workdir (unstaged changes)
        let diff = self
            .repo
            .diff_index_to_workdir(None, None)
            .map_err(|e| RepositoryError::Git(e.to_string()))?;

        // We can't easily print diff to string with git2 without a callback.
        // For now, let's just return stats.
        let stats = diff
            .stats()
            .map_err(|e| RepositoryError::Git(e.to_string()))?;
        let buf = stats
            .to_buf(git2::DiffStatsFormat::FULL, 80)
            .map_err(|e| RepositoryError::Git(e.to_string()))?;

        Ok(std::str::from_utf8(&buf)
            .unwrap_or("Invalid UTF-8")
            .to_string())
    }
}
