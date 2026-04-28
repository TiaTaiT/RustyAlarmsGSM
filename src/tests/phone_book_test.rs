// src/tests/phone_book_test.rs
use crate::phone_book::{PhoneBook};
use crate::constants::MAX_PHONE_LENGTH;

#[test]
fn phone_book_adds_and_retrieves_numbers() {
    let mut book = PhoneBook::new();

    book.add_number("+905551112233").unwrap();
    book.add_number("+905554445566").unwrap();

    assert_eq!(book.get_first(), Some("+905551112233"));
    assert_eq!(book.get(1), Some("+905554445566"));
    assert!(book.contains("+905551112233"));
}

#[test]
fn phone_book_rejects_duplicates_and_too_long_numbers() {
    let mut book = PhoneBook::new();
    let too_long = "1".repeat(MAX_PHONE_LENGTH);

    book.add_number("12345").unwrap();

    assert_eq!(book.add_number("12345"), Err("Phone number already exists"));
    assert_eq!(book.add_number(&too_long), Err("Phone number too long"));
}

#[test]
fn phone_book_stops_accepting_numbers_when_full() {
    let mut book = PhoneBook::new();

    for i in 0..8 {
        book.add_number(&format!("555000{i}")).unwrap();
    }

    assert_eq!(book.add_number("5559999"), Err("Phone book full"));
}