use lizzie_core::sanitize::{sanitize, FAILURE_MARKER};

// Observable PersistenceSanitizerTest expectations. Java-only ExportSanitizer
// object/URL policy tests are out of scope; Utils.splitCommand assertions are
// replaced with their explicit resulting string inputs below.
fn assert_cases(cases: &[(&str, &str)]) {
    for &(input, expected) in cases {
        assert_eq!(sanitize(input), expected, "input: {input:?}");
    }
}

#[test]
fn quoted_flags_and_control_characters_use_launch_argument_boundaries() {
    for flag in ["-pw", "\"-pw\"", "'-pw'"] {
        for value in ["CANARY&SUFFIX;END", "CANARY\tSUFFIX", "CANARY\nSUFFIX"] {
            assert_cases(&[(
                &format!("plink {flag} {value} -P 22"),
                &format!("plink {flag} <redacted> -P 22"),
            )]);
        }
    }
}

#[test]
fn plink_passwords_are_whole_arguments_rather_than_query_values() {
    for value in [
        "CANARY_PW_SIMPLE",
        "\"CANARY_PW_SPACE tail\"",
        "'CANARY_PW_SINGLE tail'",
        "CANARY_PW_SPECIAL&suffix;end,rest}final",
        "\"CANARY_PW_ESCAPED\\\" suffix\"",
        "CANARY_PW_PREFIX\" spaced suffix\"",
        "CANARY_PW_ESCAPED\\ suffix",
    ] {
        assert_cases(&[(
            &format!("command=plink user@host -P 22 -pw {value} ./start-engine.sh"),
            "command=plink user@host -P 22 -pw <redacted> ./start-engine.sh",
        )]);
    }
    assert_cases(&[("plink -pwfile file.txt -P 22", "plink -pwfile file.txt -P 22")]);
}

#[test]
fn ordinary_credential_fields_preserve_following_diagnostic_context() {
    assert_cases(&[(
        "token=CANARY_TOKEN password=CANARY_PASSWORD stage=install status=failed",
        "token=<redacted> password=<redacted> stage=install status=failed",
    )]);
}

#[test]
fn contribution_password_assignments_cover_raw_split_and_rendered_forms() {
    assert_cases(&[
        (
            "katago contribute -override-config \"password=CANARY_CONTRIB space&suffix;end\",\"maxSimultaneousGames=1\"",
            "katago contribute -override-config \"password=<redacted>\",\"maxSimultaneousGames=1\"",
        ),
        (
            "password=CANARY_CONTRIB space&suffix;end,maxSimultaneousGames=1",
            "password=<redacted>,maxSimultaneousGames=1",
        ),
        (
            "maxSimultaneousGames=1,password=CANARY_CONTRIB space&suffix;end",
            "maxSimultaneousGames=1,password=<redacted>",
        ),
        (
            "threads=4,password=CANARY_CONTRIB space&suffix;end,maxSimultaneousGames=1",
            "threads=4,password=<redacted>,maxSimultaneousGames=1",
        ),
        (
            "\"katago\" \"contribute\" \"-override-config\" \"password=CANARY_CONTRIB space&suffix;end,maxSimultaneousGames=1\"",
            "\"katago\" \"contribute\" \"-override-config\" \"password=<redacted>,maxSimultaneousGames=1\"",
        ),
        (
            "-override-config \"maxSimultaneousGames=1,rules=chinese\"",
            "-override-config \"maxSimultaneousGames=1,rules=chinese\"",
        ),
        (
            "-override-config \"maxSimultaneousGames=1\",\"rules=chinese\"",
            "-override-config \"maxSimultaneousGames=1\",\"rules=chinese\"",
        ),
    ]);
}

#[test]
fn contribution_password_supports_spaces_punctuation_and_quote_escapes() {
    assert_cases(&[
        (r#""password=CANARY\"quote suffix""#, r#""password=<redacted>""#),
        (r#""password=CANARY'single quote suffix""#, r#""password=<redacted>""#),
        ("'password=CANARY single quote suffix'", "'password=<redacted>'"),
        (r"'password=CANARY\'single quote suffix'", "'password=<redacted>'"),
        (r#"'password=CANARY\"double quote suffix'"#, "'password=<redacted>'"),
        (r#""password=CANARY\\path suffix""#, r#""password=<redacted>""#),
        (
            "\"password=CANARY space&suffix;end#foo!bar?baz=123\"",
            "\"password=<redacted>\"",
        ),
        (
            "password=CANARY space&suffix;end#foo!bar?baz=123,maxSimultaneousGames=1",
            "password=<redacted>,maxSimultaneousGames=1",
        ),
        (
            "maxSimultaneousGames=1,password=CANARY space&suffix;end#foo!bar?baz=123",
            "maxSimultaneousGames=1,password=<redacted>",
        ),
    ]);
}

#[test]
fn pw_assignments_and_arguments_do_not_break_pw_flags() {
    assert_cases(&[
        (
            "katago contribute -override-config \"pw=CANARY_PW space&suffix;end\",\"maxSimultaneousGames=1\"",
            "katago contribute -override-config \"pw=<redacted>\",\"maxSimultaneousGames=1\"",
        ),
        (
            "\"katago\" \"contribute\" \"-override-config\" \"pw=CANARY_PW space&suffix;end,maxSimultaneousGames=1\"",
            "\"katago\" \"contribute\" \"-override-config\" \"pw=<redacted>,maxSimultaneousGames=1\"",
        ),
        (
            "pw=CANARY_PW space&suffix;end,maxSimultaneousGames=1",
            "pw=<redacted>,maxSimultaneousGames=1",
        ),
        (
            "maxSimultaneousGames=1,pw=CANARY_PW space&suffix;end",
            "maxSimultaneousGames=1,pw=<redacted>",
        ),
        ("plink -pw \"CANARY_PW_SPACE tail\" -P 22", "plink -pw <redacted> -P 22"),
        ("plink -pwfile file.txt -P 22", "plink -pwfile file.txt -P 22"),
    ]);
}

#[test]
fn encoded_credential_forms_are_redacted_without_copying_canaries() {
    assert_cases(&[
        ("Bearer CANARY_BARE_BEARER_01", "Bearer <redacted>"),
        (
            r#"payload={\"password\":\"CANARY_ESCAPED_PASSWORD_02\"}"#,
            r#"payload={"password":"<redacted>"}"#,
        ),
        (
            r"Authorization\u003a Bearer CANARY_UNICODE_COLON_03",
            "Authorization: <redacted>",
        ),
        (r"password\u003dCANARY_UNICODE_EQUAL_04", "password=<redacted>"),
        (
            r#"payload={"password":"prefix\"CANARY_RAW_QUOTED_SUFFIX_05"}"#,
            r#"payload={"password":"<redacted>"}"#,
        ),
        (
            r#"payload={\"password\":\"prefix\\\"CANARY_EMBEDDED_QUOTED_SUFFIX_06\"}"#,
            r#"payload={"password":"<redacted>"}"#,
        ),
        (r"passw\u006frd=CANARY_UNICODE_NAME_07", "password=<redacted>"),
        (r"passw\\u006frd=CANARY_DOUBLE_UNICODE_NAME_08", "password=<redacted>"),
        (
            "body=password%3DCANARY_PERCENT_PASSWORD_09",
            "body=password%3D<redacted>",
        ),
        ("body=%70assword=CANARY_PERCENT_KEY_10", "body=%70assword=<redacted>"),
        ("body=token%253DCANARY_PERCENT_DOUBLE_11", "body=token%253D<redacted>"),
        (
            "body=passw%4Frd%3dCANARY_PERCENT_MIXED_HEX_12",
            "body=passw%4Frd%3d<redacted>",
        ),
        (
            "body=password%252525253DCANARY_PERCENT_OVERDEPTH_ASSIGNMENT_13",
            "body=password%252525253D<redacted>",
        ),
        (
            "body=%2525252570assword=CANARY_PERCENT_OVERDEPTH_KEY_14",
            "body=%2525252570assword=<redacted>",
        ),
        (
            "body=passw%254Frd%252525253dCANARY_PERCENT_OVERDEPTH_MIXED_HEX_15",
            "body=passw%254Frd%252525253d<redacted>",
        ),
        (
            "Authorization%252525253A Bearer CANARY_PERCENT_OVERDEPTH_HEADER_16",
            "Authorization%252525253A <redacted>",
        ),
        (
            "engine.exe --api-key CANARY_CLI_API_KEY gtp",
            "engine.exe --api-key <redacted> gtp",
        ),
        (
            "engine.exe --api-key 'CANARY_CLI_QUOTED_KEY with spaces' gtp",
            "engine.exe --api-key <redacted> gtp",
        ),
        (
            "body=opaque%2525252520token=CANARY_PERCENT_OVERDEPTH_BOUNDARY_17",
            "body=opaque%2525252520token=<redacted>",
        ),
    ]);
}

#[test]
fn percent_matching_preserves_encoded_prefix_and_never_rewrites_value() {
    assert_cases(&[
        (
            "body=password%3DCANARY_PERCENT_PASSWORD_09",
            "body=password%3D<redacted>",
        ),
        ("body=%70assword=CANARY_PERCENT_KEY_10", "body=%70assword=<redacted>"),
        ("body=token%253DCANARY_PERCENT_DOUBLE_11", "body=token%253D<redacted>"),
        (
            "body=passw%4Frd%3dCANARY_PERCENT_MIXED_HEX_12",
            "body=passw%4Frd%3d<redacted>",
        ),
        (
            "body=secret%2525253DCANARY_PERCENT_MAX_DEPTH",
            "body=secret%2525253D<redacted>",
        ),
        (
            "body=password%252525253DCANARY_PERCENT_OVERDEPTH_ASSIGNMENT_13",
            "body=password%252525253D<redacted>",
        ),
        (
            "body=%2525252570assword=CANARY_PERCENT_OVERDEPTH_KEY_14",
            "body=%2525252570assword=<redacted>",
        ),
        (
            "body=passw%254Frd%252525253dCANARY_PERCENT_OVERDEPTH_MIXED_HEX_15",
            "body=passw%254Frd%252525253d<redacted>",
        ),
        (
            "Authorization%252525253A Bearer CANARY_PERCENT_OVERDEPTH_HEADER_16",
            "Authorization%252525253A <redacted>",
        ),
        (
            "body=opaque%2525252520token=CANARY_PERCENT_OVERDEPTH_BOUNDARY_17",
            "body=opaque%2525252520token=<redacted>",
        ),
        (
            r#"body=%22password%22%3A"two words"&stage=done"#,
            r#"body=%22password%22%3A"<redacted>"&stage=done"#,
        ),
        (
            "body=token%3D'escaped\\' suffix'&stage=done",
            "body=token%3D'<redacted>'&stage=done",
        ),
        ("body=token%3D\"unterminated suffix", "body=token%3D\"<redacted>\""),
        ("Cookie%3A a=x; b=y\r\nstage=done", "Cookie%3A <redacted>\r\nstage=done"),
        ("body=token%3D&stage=done", "body=token%3D&stage=done"),
    ]);
}

#[test]
fn percent_scan_limit_fails_closed_without_exposing_the_value() {
    let at_scan_limit = format!("%{}3D", "25".repeat(63));
    let beyond_scan_limit = format!("%{}3D", "25".repeat(64));
    assert_cases(&[
        (
            &format!("body=token{at_scan_limit}CANARY"),
            &format!("body=token{at_scan_limit}<redacted>"),
        ),
        (
            &format!("body=token{beyond_scan_limit}CANARY_PERCENT_BEYOND_SCAN_LIMIT_18"),
            FAILURE_MARKER,
        ),
        (&format!("opaque={beyond_scan_limit}CANARY"), FAILURE_MARKER),
    ]);
}

#[test]
fn noncredential_percent_encoding_is_preserved_and_url_policy_is_unchanged() {
    for ordinary in [
        "progress=90%25 note=hello%20world code=%7Bplain%7D",
        "opaque=%2525252541",
        "https://example.invalid/a%2Fb?q=keep%20me",
    ] {
        assert_cases(&[(ordinary, ordinary)]);
    }
}

#[test]
fn credential_syntax_and_names_cover_all_persistence_patterns() {
    assert_cases(&[
        ("", ""),
        (
            "ordinary engine output stage=install status=failed",
            "ordinary engine output stage=install status=failed",
        ),
        ("--password \"two words\" gtp", "--password <redacted> gtp"),
        ("-token secret gtp", "-token <redacted> gtp"),
        ("/pw x gtp", "/pw <redacted> gtp"),
        (r#"{"password":"x"}"#, r#"{"password":"<redacted>"}"#),
        ("password=x", "password=<redacted>"),
        ("a,token=x,b", "a,token=<redacted>,b"),
        (
            "Authorization: Bearer abc\nnext=visible",
            "Authorization: <redacted>\nnext=visible",
        ),
        (
            "Proxy-Authorization = Unknown scheme credential",
            "Proxy-Authorization = <redacted>",
        ),
        ("Cookie: a=x; b=y", "Cookie: <redacted>"),
        ("Set-Cookie: a=x; HttpOnly", "Set-Cookie: <redacted>"),
        ("https://user:pass@host/path", "https://<redacted>@host/path"),
        (
            "https://host/path?api_key=x&stage=done",
            "https://host/path?api_key=<redacted>&stage=done",
        ),
        (
            "https://host/path?key=x&stage=done",
            "https://host/path?key=<redacted>&stage=done",
        ),
        ("Bearer abc", "Bearer <redacted>"),
        ("Basic dXNlcg==", "Basic <redacted>"),
        (
            r"\u0022password\u0022\u003a\u0022two words\u0022",
            r#""password":"<redacted>""#,
        ),
        (
            r"runLabel=run$archive\segment,password=CANARY secret,maxSimultaneousGames=1",
            r"runLabel=run$archive\segment,password=<redacted>,maxSimultaneousGames=1",
        ),
    ]);
    for name in [
        "pw",
        "password",
        "passwd",
        "token",
        "secret",
        "connectPassword",
        "zhizi-account-token",
        "zz-socketio-token",
        "machinekey",
        "machine-key",
        "machine_key",
        "apikey",
        "api-key",
        "api_key",
        "xapikey",
        "x-api-key",
        "x_api_key",
        "accesstoken",
        "access-token",
        "access_token",
        "xaccesstoken",
        "x-access-token",
        "x_access_token",
        "refreshtoken",
        "refresh-token",
        "refresh_token",
        "clientsecret",
        "client-secret",
        "client_secret",
    ] {
        assert_cases(&[
            (&format!("{name}=CANARY"), &format!("{name}=<redacted>")),
            (
                &format!("--{name} \"two words\" gtp"),
                &format!("--{name} <redacted> gtp"),
            ),
            (
                &format!("{}=CANARY", name.to_ascii_uppercase()),
                &format!("{}=<redacted>", name.to_ascii_uppercase()),
            ),
        ]);
    }
}

#[test]
fn canonicalization_is_bounded_and_preserves_unrecognized_escapes() {
    let slashes = "\\".repeat(1 << 16);
    assert_cases(&[
        (&format!("ordinary={slashes}u0041"), FAILURE_MARKER),
        (r"ordinary=\u2603", r"ordinary=\u2603"),
        (r"ordinary=\u00zz", r"ordinary=\u00zz"),
        (r"ordinary=\path", r"ordinary=\path"),
        ("分析 %E4%B8%AD stage=done", "分析 %E4%B8%AD stage=done"),
    ]);
}
