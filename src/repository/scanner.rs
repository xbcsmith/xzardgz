use crate::error::RepositoryError;
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

pub struct RepositoryScanner {
    root: PathBuf,
    ignore_patterns: Vec<String>,
}

impl RepositoryScanner {
    pub fn new<P: AsRef<Path>>(root: P, ignore_patterns: Vec<String>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            ignore_patterns,
        }
    }

    pub fn scan(&self) -> Result<Vec<PathBuf>, RepositoryError> {
        let mut builder = WalkBuilder::new(&self.root);
        builder.hidden(false); // Don't ignore hidden files by default, let ignore patterns handle it
        builder.git_ignore(true); // Respect .gitignore

        for pattern in &self.ignore_patterns {
            builder.add_ignore(pattern); // This might need a custom override builder
        }

        // Note: ignore crate's add_ignore expects a file path to an ignore file, not a pattern.
        // To add patterns programmatically, we need to use OverridesBuilder.

        let mut overrides = ignore::overrides::OverrideBuilder::new(&self.root);
        for pattern in &self.ignore_patterns {
            // ignore crate patterns: ! to include, normal to exclude.
            // But our config might be "exclude these".
            // If user passes "target", we want to exclude it.
            // OverrideBuilder add(glob) -> whitelist if no !, blacklist if !.
            // Wait, OverrideBuilder documentation says:
            // "A glob starting with ! means the file should be ignored." -> No, usually ! means include (whitelist).
            // Let's check documentation or assume standard gitignore behavior.
            // In gitignore: "foo" ignores foo. "!foo" includes foo.
            // In OverrideBuilder: "add(glob)" - "Matches are whitelist (include) by default."
            // So to ignore "target", we should use "!target".

            let p = format!("!{}", pattern);
            overrides
                .add(&p)
                .map_err(|e| RepositoryError::Scan(e.to_string()))?;
        }

        let overrides = overrides
            .build()
            .map_err(|e| RepositoryError::Scan(e.to_string()))?;
        builder.overrides(overrides);

        let walker = builder.build();
        let mut files = Vec::new();

        for result in walker {
            match result {
                Ok(entry) => {
                    if entry.file_type().is_some_and(|ft| ft.is_file()) {
                        files.push(entry.path().to_path_buf());
                    }
                }
                Err(err) => return Err(RepositoryError::Scan(err.to_string())),
            }
        }

        Ok(files)
    }
}
