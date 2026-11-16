use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::Deserialize;

use crate::http::HttpClient;

const DEFAULT_KEYS_URL: &str = "https://api.minecraftservices.com/publickeys";

#[derive(Deserialize)]
struct KeyData {
    #[serde(rename = "publicKey")]
    public_key: String,
}

#[derive(Deserialize)]
struct KeySet {
    #[serde(rename = "playerCertificateKeys", default)]
    player_certificate_keys: Vec<KeyData>,
}

pub fn fetch_profile_keys<C: HttpClient>(client: &C, url: Option<&str>) -> Vec<Vec<u8>> {
    let url = url.unwrap_or(DEFAULT_KEYS_URL);
    let response = match client.get(url) {
        Ok(response) if response.status == 200 => response,
        Ok(response) => {
            tracing::warn!("Services public keys request answered {}", response.status);
            return Vec::new();
        }
        Err(error) => {
            tracing::warn!("Could not fetch the Services public keys: {error}");
            return Vec::new();
        }
    };
    match serde_json::from_str::<KeySet>(&response.body) {
        Ok(set) => set
            .player_certificate_keys
            .iter()
            .filter_map(|key| STANDARD.decode(key.public_key.trim()).ok())
            .collect(),
        Err(error) => {
            tracing::warn!("Could not read the Services public keys: {error}");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpResponse;

    struct Canned(Result<HttpResponse, String>);

    impl HttpClient for Canned {
        fn get(&self, _url: &str) -> Result<HttpResponse, String> {
            match &self.0 {
                Ok(response) => Ok(HttpResponse {
                    status: response.status,
                    body: response.body.clone(),
                }),
                Err(error) => Err(error.clone()),
            }
        }
    }

    fn ok(body: &str) -> Canned {
        Canned(Ok(HttpResponse {
            status: 200,
            body: body.to_owned(),
        }))
    }

    #[test]
    fn only_the_player_certificate_keys_are_kept() {
        let body = r#"{"profilePropertyKeys":[{"publicKey":"AAAA"}],"playerCertificateKeys":[{"publicKey":"AQID"},{"publicKey":"BAUG"}]}"#;
        assert_eq!(
            fetch_profile_keys(&ok(body), None),
            vec![vec![1, 2, 3], vec![4, 5, 6]]
        );
    }

    #[test]
    fn a_failing_or_malformed_answer_yields_no_keys() {
        assert!(fetch_profile_keys(&Canned(Err("down".to_owned())), None).is_empty());
        assert!(fetch_profile_keys(&ok("not json"), None).is_empty());
        let refused = Canned(Ok(HttpResponse {
            status: 500,
            body: String::new(),
        }));
        assert!(fetch_profile_keys(&refused, None).is_empty());
    }
}
