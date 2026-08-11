use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use crate::Error;

const MAX_AGENTS_BYTES: u64 = 32 * 1024;

pub fn load_agents(workdir: &Path) -> Result<String, Error> {
    let path = workdir.join("AGENTS.md");
    let metadata = match fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(String::new()),
        Err(error) => return Err(Error::Other(format!("{}: {error}", path.display()))),
    };
    if metadata.len() > MAX_AGENTS_BYTES {
        return Err(Error::Other(format!(
            "{}: exceeds 32 KiB limit ({} bytes)",
            path.display(),
            metadata.len()
        )));
    }
    let bytes =
        fs::read(&path).map_err(|error| Error::Other(format!("{}: {error}", path.display())))?;
    String::from_utf8(bytes)
        .map_err(|error| Error::Other(format!("{}: invalid UTF-8: {error}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_only_workdir_agents_and_enforces_limit() {
        let root = tempfile::tempdir().unwrap();
        let workdir = root.path().join("child");
        fs::create_dir(&workdir).unwrap();
        fs::write(root.path().join("AGENTS.md"), "parent").unwrap();
        assert_eq!(load_agents(&workdir).unwrap(), "");

        fs::write(workdir.join("AGENTS.md"), vec![b'x'; 32 * 1024]).unwrap();
        assert_eq!(load_agents(&workdir).unwrap().len(), 32 * 1024);
        fs::write(workdir.join("AGENTS.md"), vec![b'x'; 32 * 1024 + 1]).unwrap();
        assert!(
            load_agents(&workdir)
                .unwrap_err()
                .to_string()
                .contains("32 KiB")
        );
    }
}
