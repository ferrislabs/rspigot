use super::flight::{Flight, MAX_CONCURRENT_FLIGHTS};

#[derive(Debug)]
pub struct Airborne<T> {
    pub id: i32,
    pub flight: Flight,
    pub entity: T,
}

#[derive(Debug)]
pub struct Registry<T> {
    items: Vec<Airborne<T>>,
}

impl<T> Default for Registry<T> {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}

impl<T> Registry<T> {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn insert(&mut self, id: i32, flight: Flight, entity: T) {
        self.items.push(Airborne { id, flight, entity });
    }

    pub fn extend_capped(&mut self, items: impl IntoIterator<Item = Airborne<T>>) {
        let room = MAX_CONCURRENT_FLIGHTS.saturating_sub(self.items.len());
        self.items.extend(items.into_iter().take(room));
    }

    pub fn get_mut(&mut self, id: i32) -> Option<&mut Airborne<T>> {
        self.items.iter_mut().find(|airborne| airborne.id == id)
    }

    pub fn take(&mut self, id: i32) -> Option<Airborne<T>> {
        let index = self.items.iter().position(|airborne| airborne.id == id)?;
        Some(self.items.remove(index))
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn handles(&self) -> Vec<(i32, T)>
    where
        T: Clone,
    {
        self.items
            .iter()
            .map(|airborne| (airborne.id, airborne.entity.clone()))
            .collect()
    }

    pub fn drain(&mut self) -> Vec<Airborne<T>> {
        std::mem::take(&mut self.items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{PlayerId, SheepKind, TeamId};

    fn flight() -> Flight {
        Flight::new(SheepKind::Frozen, TeamId::Red, PlayerId(1))
    }

    fn registry() -> Registry<&'static str> {
        let mut registry = Registry::default();
        registry.insert(1, flight(), "one");
        registry.insert(2, flight(), "two");
        registry
    }

    #[test]
    fn a_flight_has_a_single_owner() {
        let mut registry = registry();
        assert_eq!(
            registry.take(1).map(|airborne| airborne.entity),
            Some("one")
        );
        assert!(registry.take(1).is_none());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn the_death_handler_then_the_poller_reward_at_most_once() {
        let mut registry = registry();
        let from_death = registry.take(1);
        let from_poller = registry.take(1);
        assert_eq!(
            usize::from(from_death.is_some()) + usize::from(from_poller.is_some()),
            1
        );
    }

    #[test]
    fn the_poller_then_the_death_handler_reward_at_most_once() {
        let mut registry = registry();
        let from_poller = registry.take(2);
        let from_death = registry.take(2);
        assert_eq!(
            usize::from(from_death.is_some()) + usize::from(from_poller.is_some()),
            1
        );
    }

    #[test]
    fn a_hit_is_recorded_on_the_flight_and_keeps_it() {
        let mut registry = registry();
        let hit = registry.get_mut(2).map(|airborne| {
            airborne.flight.record_hit(PlayerId(5), 9);
            airborne.flight.last_hitter
        });
        assert_eq!(hit, Some(Some(PlayerId(5))));
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn unknown_ids_are_not_flights() {
        let mut registry = registry();
        assert!(registry.get_mut(9).is_none());
        assert!(registry.take(9).is_none());
    }

    #[test]
    fn handles_list_every_flight_and_drain_empties() {
        let mut registry = registry();
        assert_eq!(registry.handles(), vec![(1, "one"), (2, "two")]);
        assert_eq!(registry.drain().len(), 2);
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn extend_respects_the_cap() {
        let mut registry = registry();
        let room = MAX_CONCURRENT_FLIGHTS - registry.len();
        let extra = (0..room as i32 + 5).map(|id| Airborne {
            id: 100 + id,
            flight: flight(),
            entity: "x",
        });
        registry.extend_capped(extra);
        assert_eq!(registry.len(), MAX_CONCURRENT_FLIGHTS);
    }
}
