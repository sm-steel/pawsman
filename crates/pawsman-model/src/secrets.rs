//! Which value a declared secret takes.

use secrecy::SecretString;

/// The value of secret `name`: process `PAWSMAN_SECRET_<NAME>` first, then `.env`.
#[must_use]
pub fn resolve(
    name: &str,
    process: &dyn Fn(&str) -> Option<String>,
    dotenv: &[(String, SecretString)],
) -> Option<SecretString> {
    let _ = (name, process, dotenv);
    None
}

/// The process variable that holds secret `name`.
#[must_use]
pub fn env_var(name: &str) -> String {
    let _ = name;
    String::new()
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret as _;

    use super::*;

    fn dotenv() -> Vec<(String, SecretString)> {
        vec![("token".into(), SecretString::from("from-dotenv"))]
    }

    #[test]
    fn process_env_wins_over_dotenv() {
        let process = |k: &str| (k == "PAWSMAN_SECRET_TOKEN").then(|| "from-process".to_owned());

        let value = resolve("token", &process, &dotenv());

        assert_eq!(
            value.as_ref().map(|v| v.expose_secret()),
            Some("from-process")
        );
    }

    #[test]
    fn dotenv_is_the_fallback() {
        let value = resolve("token", &|_| None, &dotenv());

        assert_eq!(
            value.as_ref().map(|v| v.expose_secret()),
            Some("from-dotenv")
        );
    }

    #[test]
    fn name_is_mapped_to_an_env_var() {
        assert_eq!(env_var("client-secret"), "PAWSMAN_SECRET_CLIENT_SECRET");
        assert_eq!(env_var("a.b9"), "PAWSMAN_SECRET_A_B9");
    }

    #[test]
    fn unset_everywhere_is_none() {
        assert!(resolve("other", &|_| None, &dotenv()).is_none());
    }
}
