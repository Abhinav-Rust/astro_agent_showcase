pub fn sanitize_filename(input: &str) -> String {
    input
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("John Doe"), "John_Doe");
        assert_eq!(sanitize_filename("user@name!"), "user_name_");
        assert_eq!(sanitize_filename("123 test"), "123_test");
    }
}
