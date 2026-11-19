use crate::domain::RoundEvent;

use super::view::{team_name, team_prefix};

pub fn message(event: &RoundEvent) -> Option<String> {
    Some(match event {
        RoundEvent::CountdownStarted => "§eLa partie va bientôt commencer !".to_owned(),
        RoundEvent::CountdownCancelled => {
            "§cPas assez de joueurs, le compte à rebours est annulé.".to_owned()
        }
        RoundEvent::Started => "§aLa partie commence ! Lancez vos moutons !".to_owned(),
        RoundEvent::SheepDue(team) => format!(
            "§eMouton pour l'équipe {}{} §e!",
            team_prefix(*team),
            team_name(*team)
        ),
        RoundEvent::Respawn(_) => "§eVous avez perdu une vie et réapparaissez.".to_owned(),
        RoundEvent::Eliminated(_) => "§cUn joueur a été éliminé !".to_owned(),
        RoundEvent::Won(team) => format!(
            "§6Victoire de l'équipe {}{} §6!",
            team_prefix(*team),
            team_name(*team)
        ),
        RoundEvent::Draw => "§6Égalité, personne ne gagne.".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{PlayerId, TeamId};

    fn every_event() -> Vec<RoundEvent> {
        let mut events = vec![
            RoundEvent::CountdownStarted,
            RoundEvent::CountdownCancelled,
            RoundEvent::Started,
            RoundEvent::Respawn(PlayerId(1)),
            RoundEvent::Eliminated(PlayerId(1)),
            RoundEvent::Draw,
        ];
        for team in TeamId::ALL {
            events.push(RoundEvent::SheepDue(team));
            events.push(RoundEvent::Won(team));
        }
        events
    }

    #[test]
    fn every_event_has_a_non_empty_french_message() {
        for event in every_event() {
            let text = message(&event);
            assert!(text.is_some_and(|text| !text.is_empty()), "{event:?}");
        }
    }

    #[test]
    fn team_messages_name_the_team() {
        let sheep = message(&RoundEvent::SheepDue(TeamId::Red)).unwrap_or_default();
        assert!(sheep.contains("Rouge"));
        let won = message(&RoundEvent::Won(TeamId::Blue)).unwrap_or_default();
        assert!(won.contains("Bleue"));
    }
}
