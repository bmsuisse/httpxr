
#[test]
fn test_port_extraction_8000() {
    let url = "http://127.0.0.1:8000/";
    let parsed = urlparse_impl(url, None).unwrap();
    println!("Parsed: {:?}", parsed);
    assert_eq!(parsed.port, Some(8000));
    assert_eq!(parsed.host, "127.0.0.1");
}
