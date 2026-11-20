use rspigot_core::client_extras::{KnownLink, LinkLabel, ResourcePackOffer, ServerLink};
use rspigot_core::events::ClientEvent;
use rspigot_protocol::common::{
    CookieRequest, CustomReportDetails, Identifier, KnownLinkType, ResourcePackPop,
    ResourcePackPush, ServerLink as WireLink, ServerLinkLabel, ServerLinks, StoreCookie,
};
use rspigot_protocol::ids::play::clientbound as play;
use rspigot_protocol::play::clientbound::StartConfiguration;
use rspigot_protocol::types::Text;

use crate::connection::ConnectionError;
use crate::encode::PacketSink;
use crate::text::to_nbt;

fn known(link: KnownLink) -> KnownLinkType {
    match link {
        KnownLink::BugReport => KnownLinkType::BugReport,
        KnownLink::CommunityGuidelines => KnownLinkType::CommunityGuidelines,
        KnownLink::Support => KnownLinkType::Support,
        KnownLink::Status => KnownLinkType::Status,
        KnownLink::Feedback => KnownLinkType::Feedback,
        KnownLink::Community => KnownLinkType::Community,
        KnownLink::Website => KnownLinkType::Website,
        KnownLink::Forums => KnownLinkType::Forums,
        KnownLink::News => KnownLinkType::News,
        KnownLink::Announcements => KnownLinkType::Announcements,
    }
}

pub fn wire_links(links: &[ServerLink]) -> ServerLinks {
    ServerLinks {
        links: links
            .iter()
            .map(|link| WireLink {
                label: match &link.label {
                    LinkLabel::Known(kind) => ServerLinkLabel::Known(known(*kind)),
                    LinkLabel::Custom(text) => ServerLinkLabel::Custom(Text(to_nbt(text))),
                },
                url: link.url.clone(),
            })
            .collect(),
    }
}

pub fn wire_pack(offer: &ResourcePackOffer) -> ResourcePackPush {
    ResourcePackPush {
        id: offer.id,
        url: offer.url.clone(),
        hash: offer.hash.clone(),
        required: offer.required,
        prompt: offer.prompt.as_ref().map(|text| Text(to_nbt(text))),
    }
}

pub fn encode<W: PacketSink>(event: &ClientEvent, writer: &mut W) -> Result<(), ConnectionError> {
    match event {
        ClientEvent::ResourcePackPush(offer) => {
            writer.queue(play::RESOURCE_PACK_PUSH, &wire_pack(offer))
        }
        ClientEvent::ResourcePackPop(id) => {
            writer.queue(play::RESOURCE_PACK_POP, &ResourcePackPop { id: *id })
        }
        ClientEvent::ServerLinks(links) => writer.queue(play::SERVER_LINKS, &wire_links(links)),
        ClientEvent::StoreCookie { key, payload } => writer.queue(
            play::STORE_COOKIE,
            &StoreCookie {
                key: Identifier::parse(key)?,
                payload: payload.clone(),
            },
        ),
        ClientEvent::CookieRequest(key) => writer.queue(
            play::COOKIE_REQUEST,
            &CookieRequest {
                key: Identifier::parse(key)?,
            },
        ),
        ClientEvent::ReportDetails(details) => writer.queue(
            play::CUSTOM_REPORT_DETAILS,
            &CustomReportDetails {
                details: details.clone(),
            },
        ),
        ClientEvent::Reconfigure => writer.queue(play::START_CONFIGURATION, &StartConfiguration),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use rspigot_domain::text::Component;
    use rspigot_protocol::codec::Encode;
    use uuid::Uuid;

    use super::*;

    #[derive(Default)]
    struct Captured(Vec<(i32, Vec<u8>)>);

    impl PacketSink for Captured {
        fn queue<P: Encode + ?Sized>(
            &mut self,
            id: i32,
            packet: &P,
        ) -> Result<(), ConnectionError> {
            let mut body = Vec::new();
            packet.encode(&mut body);
            self.0.push((id, body));
            Ok(())
        }

        fn queue_prebuilt(&mut self, _: &[u8]) {}
    }

    fn encoded(event: ClientEvent) -> Vec<(i32, Vec<u8>)> {
        let mut sink = Captured::default();
        encode(&event, &mut sink).unwrap();
        sink.0
    }

    #[test]
    fn a_pack_push_carries_id_url_hash_required_and_prompt() {
        let packets = encoded(ClientEvent::ResourcePackPush(ResourcePackOffer {
            id: Uuid::from_u128(1),
            url: "u".to_owned(),
            hash: "h".to_owned(),
            required: true,
            prompt: None,
        }));
        let mut body = 1u128.to_be_bytes().to_vec();
        body.extend([1, b'u', 1, b'h', 1, 0]);
        assert_eq!(packets, vec![(play::RESOURCE_PACK_PUSH, body)]);
    }

    #[test]
    fn a_pack_push_prompt_is_a_text_component() {
        let packets = encoded(ClientEvent::ResourcePackPush(ResourcePackOffer {
            id: Uuid::from_u128(1),
            url: String::new(),
            hash: String::new(),
            required: false,
            prompt: Some(Component::text("hi")),
        }));
        let body = &packets[0].1;
        assert_eq!(body[16..19], [0, 0, 0]);
        assert_eq!(body[19], 1);
        assert_eq!(body[20], 8);
    }

    #[test]
    fn a_pack_pop_carries_an_optional_id() {
        assert_eq!(
            encoded(ClientEvent::ResourcePackPop(None)),
            vec![(play::RESOURCE_PACK_POP, vec![0])]
        );
        let mut body = vec![1];
        body.extend(2u128.to_be_bytes());
        assert_eq!(
            encoded(ClientEvent::ResourcePackPop(Some(Uuid::from_u128(2)))),
            vec![(play::RESOURCE_PACK_POP, body)]
        );
    }

    #[test]
    fn server_links_carry_the_bug_report_as_a_known_label() {
        let packets = encoded(ClientEvent::ServerLinks(vec![ServerLink {
            label: LinkLabel::Known(KnownLink::BugReport),
            url: "x".to_owned(),
        }]));
        assert_eq!(packets, vec![(play::SERVER_LINKS, vec![1, 1, 0, 1, b'x'])]);
    }

    #[test]
    fn a_custom_link_label_is_a_text_component() {
        let packets = encoded(ClientEvent::ServerLinks(vec![ServerLink {
            label: LinkLabel::Custom(Component::text("Shop")),
            url: "x".to_owned(),
        }]));
        let body = &packets[0].1;
        assert_eq!(body[..3], [1, 0, 8]);
        assert_eq!(body[body.len() - 2..], [1, b'x']);
    }

    #[test]
    fn cookie_packets_carry_a_key_and_a_payload() {
        assert_eq!(
            encoded(ClientEvent::StoreCookie {
                key: "a:b".to_owned(),
                payload: vec![9, 8],
            }),
            vec![(play::STORE_COOKIE, vec![3, b'a', b':', b'b', 2, 9, 8])]
        );
        assert_eq!(
            encoded(ClientEvent::CookieRequest("a:b".to_owned())),
            vec![(play::COOKIE_REQUEST, vec![3, b'a', b':', b'b'])]
        );
    }

    #[test]
    fn report_details_are_a_map_of_strings() {
        assert_eq!(
            encoded(ClientEvent::ReportDetails(vec![(
                "k".to_owned(),
                "v".to_owned()
            )])),
            vec![(play::CUSTOM_REPORT_DETAILS, vec![1, 1, b'k', 1, b'v'])]
        );
    }

    #[test]
    fn reconfiguring_sends_the_empty_start_configuration() {
        assert_eq!(
            encoded(ClientEvent::Reconfigure),
            vec![(play::START_CONFIGURATION, Vec::new())]
        );
    }

    #[test]
    fn a_bad_cookie_key_is_an_error() {
        let mut sink = Captured::default();
        assert!(encode(&ClientEvent::CookieRequest("Bad Key".to_owned()), &mut sink).is_err());
    }
}
