use super::*;

fn config() -> MobileConfig {
    MobileConfig {
        platform: MobilePlatform::Android,
        server_url: "http://127.0.0.1:4723".into(),
        session_id: "session-1".into(),
    }
}

#[test]
fn rejects_session_path_injection() {
    for id in [
        "",
        "../other",
        "abc?command=click",
        "id#fragment",
        "id\n",
        "a/b",
        "id%2fother",
    ] {
        let mut config = config();
        config.session_id = id.into();
        assert_eq!(config.validate().unwrap_err().code, ErrorCode::InvalidArgs);
    }
}

#[test]
fn rejects_credentials_and_non_http_endpoints() {
    for url in [
        "file:///tmp/socket",
        "http://",
        "http:///path",
        "https://user:password@localhost",
        "http://localhost?query=1",
        "http://localhost/#token",
        "http://localhost\n",
    ] {
        let mut config = config();
        config.server_url = url.into();
        assert_eq!(config.validate().unwrap_err().code, ErrorCode::InvalidArgs);
    }
}

#[test]
fn accepts_remote_tls_and_custom_appium_base_path() {
    for url in [
        "http://127.0.0.1:4723",
        "http://[::1]:4723/wd/hub/",
        "https://devices.example.com/appium",
    ] {
        let mut config = config();
        config.server_url = url.into();
        assert!(config.validate().is_ok());
    }
}

#[test]
fn parses_only_mobile_platform_names() {
    assert_eq!(
        MobilePlatform::parse("Android").unwrap(),
        MobilePlatform::Android
    );
    assert_eq!(MobilePlatform::parse("iOS").unwrap(), MobilePlatform::Ios);
    for value in ["", "macos", "android "] {
        assert!(MobilePlatform::parse(value).is_err());
    }
}
