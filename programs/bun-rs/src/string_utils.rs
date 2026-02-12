/// Values type alias for a vec of serde_json values
pub type Values = Vec<serde_json::Value>;

/// # Swap Remove `Value`
///
/// Remove a value from a serde_json `Values` array, and take ownership
/// of it in a fast way by swapping in the final value of the array.
///
///```rust
/// use bun_rs::string_utils::swap_remove_value;
/// use serde_json::json;
///
/// let mut values = vec![
///  json!("@types/bun@1.2.4"),
///  json!({}),
///  json!([]),
///  json!("sha512-QtuV5OMR8/rdKJs213iwXDpfVvnskPXY/S0ZiFbsTjQZycuqPbMW8Gf/XhLfwE5njW8sxI2WjISURXPlHypMFA==")
/// ];
///
/// assert_eq!(
///     swap_remove_value(&mut values, 0),
///     "@types/bun@1.2.4"
/// );
/// assert_eq!(
///     swap_remove_value(&mut values, 0),
///     "sha512-QtuV5OMR8/rdKJs213iwXDpfVvnskPXY/S0ZiFbsTjQZycuqPbMW8Gf/XhLfwE5njW8sxI2WjISURXPlHypMFA=="
/// );
/// ```
pub fn swap_remove_value(values: &mut Values, index: usize) -> String {
    let mut value = values.swap_remove(index).to_string();

    assert!(
        value.starts_with('"'),
        "Value should start with a quote: {value:?}"
    );
    assert!(
        value.ends_with('"'),
        "Value should end with a quote: {value:?}"
    );

    value.drain(1..value.len() - 1).collect()
}

/// # Split Once (Owned)
///
/// Variant of `String::split_once` which consumes the original string and produces
/// two owned values as an output (without a new allocation).
///
///```rust
/// use bun_rs::string_utils::split_once_owned;
///
/// let input = "hello#world".to_owned();
///
/// assert_eq!(
///     split_once_owned(input, '#'),
///     Some(("hello".to_owned(), "world".to_owned()))
/// );
/// ```
pub fn split_once_owned(mut input: String, char: char) -> Option<(String, String)> {
    let split_pos = input.find(char)?;

    let mut first: String = input.drain(..=split_pos).collect();
    first.pop();

    Some((first, input))
}

/// # Drop Prefix
///
/// Consumes an owned string with a known prefix and returns an owned
/// value without that prefix (reuses the old allocation).
///
///```rust
/// use bun_rs::string_utils::drop_prefix;
///
/// let input = "hello:world".to_owned();
///
/// assert_eq!(
///     drop_prefix(input, "hello:"),
///     "world"
/// );
/// ```
pub fn drop_prefix(mut input: String, prefix: &str) -> String {
    if input.starts_with(prefix) {
        input.drain(..prefix.len());
    }

    input
}
