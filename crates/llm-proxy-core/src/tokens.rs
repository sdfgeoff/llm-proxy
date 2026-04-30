pub fn estimate_tokens_from_chars(text: &str) -> u64 {
    let chars = text.chars().count() as u64;
    chars.div_ceil(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_with_ceiling() {
        assert_eq!(estimate_tokens_from_chars(""), 0);
        assert_eq!(estimate_tokens_from_chars("a"), 1);
        assert_eq!(estimate_tokens_from_chars("abcd"), 1);
        assert_eq!(estimate_tokens_from_chars("abcde"), 2);
    }
}
