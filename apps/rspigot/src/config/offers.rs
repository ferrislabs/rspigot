use md5::{Digest, Md5};
use rspigot_core::client_extras::{
    KnownLink, LinkLabel, ResourcePackOffer, ServerLink, parse_text_component,
};
use uuid::Uuid;

use super::{Properties, YamlConfig};

pub const SERVER_LINKS_PATH: &str = "server-links";

fn name_uuid(text: &str) -> Uuid {
    let digest: [u8; 16] = Md5::digest(text.as_bytes()).into();
    uuid::Builder::from_md5_bytes(digest).into_uuid()
}

fn valid_sha1(hash: &str) -> bool {
    hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn resource_pack(properties: &Properties) -> Option<ResourcePackOffer> {
    let url = properties.string("resource-pack");
    if url.is_empty() {
        return None;
    }
    let hash = properties.string("resource-pack-sha1");
    if hash.is_empty() {
        tracing::warn!(
            "You specified a resource pack without providing a sha1 hash. Pack will be updated on the client only if you change the name of the pack."
        );
    } else if !valid_sha1(hash) {
        tracing::warn!("Invalid sha1 for resource-pack-sha1");
    }
    let id = match properties.string("resource-pack-id") {
        "" => name_uuid(url),
        text => match Uuid::parse_str(text) {
            Ok(id) => id,
            Err(_) => {
                tracing::warn!("Invalid resource-pack-id `{text}`, generating one from the url");
                name_uuid(url)
            }
        },
    };
    let prompt = match properties.string("resource-pack-prompt") {
        "" => None,
        text => {
            let parsed = parse_text_component(text);
            if parsed.is_none() {
                tracing::warn!("Invalid resource-pack-prompt, it must be a JSON text component");
            }
            parsed
        }
    };
    Some(ResourcePackOffer {
        id,
        url: url.to_owned(),
        hash: hash.to_owned(),
        required: properties.bool("require-resource-pack"),
        prompt,
    })
}

pub fn server_links(properties: &Properties, spigot: &YamlConfig) -> Vec<ServerLink> {
    let mut links = Vec::new();
    let bug_report = properties.string("bug-report-link");
    if !bug_report.is_empty() {
        links.push(ServerLink {
            label: LinkLabel::Known(KnownLink::BugReport),
            url: bug_report.to_owned(),
        });
    }
    for (label, url) in spigot.entries(SERVER_LINKS_PATH) {
        if url.is_empty() {
            continue;
        }
        let link = ServerLink::labelled(&label, &url);
        if bug_report.is_empty() || link.label != LinkLabel::Known(KnownLink::BugReport) {
            links.push(link);
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use rspigot_domain::text::Component;

    use super::*;

    fn properties(pairs: &[(&str, &str)]) -> Properties {
        let mut properties = Properties::parse("");
        for (key, value) in pairs {
            properties.set(key, value);
        }
        properties
    }

    #[test]
    fn no_url_means_no_pack() {
        assert_eq!(resource_pack(&properties(&[])), None);
    }

    #[test]
    fn the_pack_id_is_generated_from_the_url_as_vanilla() {
        let offer = resource_pack(&properties(&[("resource-pack", "a")])).unwrap();
        assert_eq!(offer.id.to_string(), "0cc175b9-c0f1-36a8-b1c3-99e269772661");
        assert_eq!(offer.id.get_version_num(), 3);
    }

    #[test]
    fn a_given_id_hash_prompt_and_requirement_are_kept() {
        let offer = resource_pack(&properties(&[
            ("resource-pack", "https://example.org/p.zip"),
            ("resource-pack-id", "00000000-0000-0000-0000-000000000007"),
            (
                "resource-pack-sha1",
                "0123456789abcdef0123456789abcdef01234567",
            ),
            ("resource-pack-prompt", "{\"text\":\"Hi\"}"),
            ("require-resource-pack", "true"),
        ]))
        .unwrap();
        assert_eq!(offer.id, Uuid::from_u128(7));
        assert_eq!(offer.hash, "0123456789abcdef0123456789abcdef01234567");
        assert!(offer.required);
        assert_eq!(offer.prompt, Some(Component::text("Hi")));
    }

    #[test]
    fn an_invalid_id_falls_back_to_the_generated_one() {
        let offer = resource_pack(&properties(&[
            ("resource-pack", "u"),
            ("resource-pack-id", "nope"),
        ]))
        .unwrap();
        assert_eq!(offer.id, name_uuid("u"));
    }

    #[test]
    fn the_bug_report_property_is_the_bug_report_link() {
        let spigot = YamlConfig::from_str("{}").unwrap();
        let links = server_links(&properties(&[("bug-report-link", "https://b")]), &spigot);
        assert_eq!(
            links,
            vec![ServerLink {
                label: LinkLabel::Known(KnownLink::BugReport),
                url: "https://b".to_owned(),
            }]
        );
        assert!(server_links(&properties(&[]), &spigot).is_empty());
    }

    #[test]
    fn spigot_links_map_known_types_and_custom_labels() {
        let spigot = YamlConfig::from_str(
            "server-links:\n  website: https://w\n  Our Shop: https://s\n  empty: ''\n",
        )
        .unwrap();
        let links = server_links(&properties(&[]), &spigot);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].label, LinkLabel::Known(KnownLink::Website));
        assert_eq!(
            links[1].label,
            LinkLabel::Custom(Component::text("Our Shop"))
        );
        assert_eq!(links[1].url, "https://s");
    }

    #[test]
    fn the_property_wins_over_a_spigot_bug_report_entry() {
        let spigot = YamlConfig::from_str("server-links:\n  bug_report: https://other\n").unwrap();
        let links = server_links(&properties(&[("bug-report-link", "https://b")]), &spigot);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "https://b");
    }
}
