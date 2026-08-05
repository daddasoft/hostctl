use std::{fs, path::Path};

#[test]
fn github_workflows_are_valid_yaml() {
    for name in [
        "ci.yml",
        "staging.yml",
        "release.yml",
        "installer-smoke.yml",
    ] {
        let path = Path::new(".github/workflows").join(name);
        let content = fs::read_to_string(&path).unwrap();
        serde_yaml::from_str::<serde_yaml::Value>(&content)
            .unwrap_or_else(|error| panic!("{} is invalid YAML: {error}", path.display()));
    }
}
