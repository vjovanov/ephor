//! Ticket keys as they appear in branches and in prose (§FS-007-matters.2).
//!
//! A ticket key is a shape, not a lookup: `ABC-42` in a branch name, in a
//! pull request title, in a sentence someone wrote. Recognizing one is pure
//! text work with no store behind it, which is why it sits in core and not in
//! the registry that reads files (§AR-001-layers.1, §AR-001-layers.3).

/// Every ticket key a piece of text names, in the order it names them. The
/// same shape [`extract_ticket`] looks for in a branch, found anywhere: what a
/// title or a message refers to is how one matter is related to another
/// (§FS-007-matters.2).
pub fn tickets_in(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let words: Vec<&str> = text
        .split(|character: char| {
            !character.is_alphanumeric() && character != '-' && character != '_'
        })
        .collect();
    for word in words {
        let ticket = extract_ticket(word);
        if !ticket.is_empty() && !found.contains(&ticket) {
            found.push(ticket);
        }
    }
    found
}

/// The ticket key a branch name carries, or empty where it carries none: an
/// upper-case word followed by digits, with `/` read the same as `-`.
pub fn extract_ticket(branch: &str) -> String {
    let normalized = branch.replace('/', "-");
    let parts: Vec<&str> = normalized.split('-').collect();
    for window in parts.windows(2) {
        let (part, next) = (window[0], window[1]);
        let is_upper = !part.is_empty()
            && part.chars().any(|c| c.is_alphabetic())
            && part
                .chars()
                .filter(|c| c.is_alphabetic())
                .all(|c| c.is_uppercase());
        let is_digit = !next.is_empty() && next.chars().all(|c| c.is_ascii_digit());
        if is_upper && is_digit {
            return format!("{part}-{next}");
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_ticket_finds_jira_keys() {
        assert_eq!(extract_ticket("you/ABC-42-retry-window"), "ABC-42");
        assert_eq!(extract_ticket("feature/no-ticket"), "");
        assert_eq!(extract_ticket(""), "");
    }

    #[test]
    fn a_sentence_names_its_tickets_in_the_order_it_names_them() {
        assert_eq!(
            tickets_in("ABC-42 is blocked by DEF-7, and ABC-42 again"),
            vec!["ABC-42".to_string(), "DEF-7".to_string()]
        );
        assert!(tickets_in("nothing to see").is_empty());
    }
}
