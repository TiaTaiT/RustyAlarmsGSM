// /src/tests/custom_strings_test.rs
use crate::custom_strings::*;

#[test]
fn test_extract_between_delimiters() {
    let s = r#"+CPBR: 2,"*105#",129,"0""#;
    assert_eq!(extract_between_delimiters(s, "\"", "\""), Some("*105#"));
    
    let invalid = r#"+CPBR: 2, *105#,129,"0""#;
    assert_eq!(extract_between_delimiters(invalid, "\"", "\""), Some("0"));
}

#[test]
fn test_separate_chars_by_commas() {
    let mut buf = [0u8; 16];
    let result = separate_chars_by_commas("123", &mut buf);
    
    assert_eq!(result, Some("1,2,3"));
    
    let mut small_buf =[0u8; 2];
    let overflow_result = separate_chars_by_commas("12", &mut small_buf);
    assert_eq!(overflow_result, None);
}