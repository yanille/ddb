//! Verifies that `ddb` honors the standard AWS shared files:
//! credentials from `~/.aws/credentials` and region/settings from
//! `~/.aws/config` (via the `AWS_SHARED_CREDENTIALS_FILE` / `AWS_CONFIG_FILE`
//! overrides, which point at the same profile-file providers used for the
//! default `~/.aws/*` paths).
//!
//! This exercises the real AWS provider chain offline (static keys in a file
//! require no network) and asserts only the non-secret access key id — never
//! the secret access key.
//!
//! All assertions live in a single test function so the process-global
//! environment is mutated sequentially, with no cross-test races.

use std::fs;
use std::path::{Path, PathBuf};

use aws_credential_types::provider::ProvideCredentials;

fn write_fixture(dir: &Path, name: &str, contents: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, contents).expect("write fixture");
    path
}

#[tokio::test]
async fn honors_shared_credentials_and_config_files() {
    let dir = std::env::temp_dir().join(format!("ddb-aws-test-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("create temp dir");

    // Shared *config* file (~/.aws/config equivalent): sets the region.
    let config_path = write_fixture(
        &dir,
        "config",
        "[default]\nregion = eu-central-1\n\n[profile analytics]\nregion = ap-south-1\n",
    );
    // Shared *credentials* file (~/.aws/credentials equivalent): sets the keys.
    // These are AWS's documentation example values, not real credentials.
    let credentials_path = write_fixture(
        &dir,
        "credentials",
        "[default]\n\
         aws_access_key_id = AKIAIOSFODNN7EXAMPLE\n\
         aws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY\n\
         \n\
         [analytics]\n\
         aws_access_key_id = AKIAI44QH8DHBEXAMPLE\n\
         aws_secret_access_key = je7MtGbClwBF/2Zp9Utk/h3yCo8nvbEXAMPLEKEY\n",
    );

    // Ensure ambient AWS_* env vars can't shadow the files we are testing.
    for key in [
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_SESSION_TOKEN",
        "AWS_PROFILE",
        "AWS_REGION",
        "AWS_DEFAULT_REGION",
    ] {
        std::env::remove_var(key);
    }
    std::env::set_var("AWS_CONFIG_FILE", &config_path);
    std::env::set_var("AWS_SHARED_CREDENTIALS_FILE", &credentials_path);
    std::env::set_var("AWS_EC2_METADATA_DISABLED", "true");

    // 1. Default profile: region from config file, credentials from creds file.
    let config = ddb::dynamodb::client::load_config(None, None).await;
    assert_eq!(
        config.region().map(|r| r.to_string()),
        Some("eu-central-1".to_string()),
        "region should be read from the shared config file"
    );
    let creds = config
        .credentials_provider()
        .expect("a credentials provider should be configured")
        .provide_credentials()
        .await
        .expect("credentials should resolve from the shared credentials file");
    assert_eq!(
        creds.access_key_id(),
        "AKIAIOSFODNN7EXAMPLE",
        "credentials should be read from the shared credentials file"
    );

    // 2. A named profile pulls that profile's region (config) and keys (creds).
    let config = ddb::dynamodb::client::load_config(Some("analytics"), None).await;
    assert_eq!(
        config.region().map(|r| r.to_string()),
        Some("ap-south-1".to_string()),
        "named profile region should come from the config file"
    );
    let creds = config
        .credentials_provider()
        .expect("provider")
        .provide_credentials()
        .await
        .expect("named profile credentials should resolve");
    assert_eq!(creds.access_key_id(), "AKIAI44QH8DHBEXAMPLE");

    // 3. An explicit --region override wins over the config file's region, while
    //    credentials still come from the credentials file.
    let config = ddb::dynamodb::client::load_config(None, Some("us-west-2")).await;
    assert_eq!(
        config.region().map(|r| r.to_string()),
        Some("us-west-2".to_string()),
        "explicit region override should win"
    );

    let _ = fs::remove_dir_all(&dir);
}
