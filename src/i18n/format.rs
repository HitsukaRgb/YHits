use super::{t, Tr};

pub fn tf(key: Tr, pairs: &[(&str, &str)]) -> String {
    let mut text = t(key).to_string();
    for (name, value) in pairs {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}
