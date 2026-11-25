use rspigot_protocol::ids;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Configuration,
    Play,
}

impl Phase {
    pub const fn after_serverbound(self, packet_id: i32) -> Self {
        match (self, packet_id) {
            (Self::Configuration, ids::configuration::serverbound::FINISH_CONFIGURATION) => {
                Self::Play
            }
            (Self::Play, ids::play::serverbound::CONFIGURATION_ACKNOWLEDGED) => Self::Configuration,
            (phase, _) => phase,
        }
    }

    pub const fn disconnect_id(self) -> i32 {
        match self {
            Self::Configuration => ids::configuration::clientbound::DISCONNECT,
            Self::Play => ids::play::clientbound::DISCONNECT,
        }
    }

    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Configuration => 0,
            Self::Play => 1,
        }
    }

    pub const fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Configuration,
            _ => Self::Play,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finishing_the_configuration_enters_play() {
        let phase = Phase::Configuration
            .after_serverbound(ids::configuration::serverbound::FINISH_CONFIGURATION);
        assert_eq!(phase, Phase::Play);
    }

    #[test]
    fn acknowledging_a_reconfiguration_goes_back_to_configuration() {
        let phase =
            Phase::Play.after_serverbound(ids::play::serverbound::CONFIGURATION_ACKNOWLEDGED);
        assert_eq!(phase, Phase::Configuration);
    }

    #[test]
    fn other_packets_keep_the_phase() {
        assert_eq!(
            Phase::Configuration.after_serverbound(ids::configuration::serverbound::CUSTOM_PAYLOAD),
            Phase::Configuration
        );
        assert_eq!(
            Phase::Play.after_serverbound(ids::configuration::serverbound::FINISH_CONFIGURATION),
            Phase::Play
        );
    }

    #[test]
    fn the_disconnect_packet_follows_the_phase() {
        assert_eq!(
            Phase::Configuration.disconnect_id(),
            ids::configuration::clientbound::DISCONNECT
        );
        assert_eq!(
            Phase::Play.disconnect_id(),
            ids::play::clientbound::DISCONNECT
        );
        for phase in [Phase::Configuration, Phase::Play] {
            assert_eq!(Phase::from_u8(phase.as_u8()), phase);
        }
    }
}
