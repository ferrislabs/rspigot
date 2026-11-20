use rspigot_blocks::BlockKind;
use rspigot_domain::BlockStateId;

const OPAQUE: u8 = 15;
const WHITE: u32 = 0xFF_F9FFFE;
const DYES: [(&str, u32); 16] = [
    ("white", 0xFF_F9FFFE),
    ("orange", 0xFF_F9801D),
    ("magenta", 0xFF_C74EBD),
    ("light_blue", 0xFF_3AB3DA),
    ("yellow", 0xFF_FED83D),
    ("lime", 0xFF_80C71F),
    ("pink", 0xFF_F38BAA),
    ("gray", 0xFF_474F52),
    ("light_gray", 0xFF_9D9D97),
    ("cyan", 0xFF_169C9C),
    ("purple", 0xFF_8932B8),
    ("blue", 0xFF_3C44AA),
    ("brown", 0xFF_835432),
    ("green", 0xFF_5E7C16),
    ("red", 0xFF_B02E26),
    ("black", 0xFF_1D1D21),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Section {
    pub(crate) color: u32,
    pub(crate) height: u32,
}

pub(crate) fn beam_color(state: BlockStateId) -> Option<u32> {
    let kind = state.kind()?;
    if kind == BlockKind::Beacon {
        return Some(WHITE);
    }
    let name = kind.name().strip_prefix("minecraft:")?;
    let dye = name
        .strip_suffix("_stained_glass_pane")
        .or_else(|| name.strip_suffix("_stained_glass"))?;
    DYES.iter()
        .find(|(known, _)| *known == dye)
        .map(|(_, color)| *color)
}

fn average(first: u32, second: u32) -> u32 {
    (0..4).fold(0, |mixed, channel| {
        let shift = channel * 8;
        let a = (first >> shift) & 0xFF;
        let b = (second >> shift) & 0xFF;
        mixed | (((a + b) / 2) << shift)
    })
}

pub(crate) fn sections(column: impl Iterator<Item = BlockStateId>) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    for state in column {
        if let Some(color) = beam_color(state) {
            if sections.len() <= 1 {
                sections.push(Section { color, height: 1 });
                continue;
            }
            match sections.last_mut() {
                Some(last) if last.color == color => last.height += 1,
                Some(last) => {
                    let mixed = average(last.color, color);
                    sections.push(Section {
                        color: mixed,
                        height: 1,
                    });
                }
                None => {}
            }
        } else {
            let blocks =
                state.light_dampening() >= OPAQUE && state.kind() != Some(BlockKind::Bedrock);
            match sections.last_mut() {
                Some(last) if !blocks => last.height += 1,
                _ => return Vec::new(),
            }
        }
    }
    sections
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(kind: BlockKind) -> BlockStateId {
        kind.default_state()
    }

    fn column(kinds: &[BlockKind], air: usize) -> Vec<BlockStateId> {
        let mut states: Vec<BlockStateId> = kinds.iter().map(|kind| state(*kind)).collect();
        states.extend(std::iter::repeat_n(BlockStateId::AIR, air));
        states
    }

    #[test]
    fn a_clear_sky_gives_one_white_section_the_height_of_the_column() {
        let found = sections(column(&[BlockKind::Beacon], 5).into_iter());
        assert_eq!(
            found,
            vec![Section {
                color: WHITE,
                height: 6
            }]
        );
    }

    #[test]
    fn the_first_glass_takes_its_own_color() {
        let found =
            sections(column(&[BlockKind::Beacon, BlockKind::RedStainedGlass], 2).into_iter());
        assert_eq!(found.len(), 2);
        assert_eq!(
            found[1],
            Section {
                color: 0xFF_B02E26,
                height: 3
            }
        );
    }

    #[test]
    fn later_glass_blends_with_the_section_before_it() {
        let found = sections(
            column(
                &[
                    BlockKind::Beacon,
                    BlockKind::RedStainedGlass,
                    BlockKind::BlueStainedGlassPane,
                ],
                0,
            )
            .into_iter(),
        );
        assert_eq!(found.len(), 3);
        assert_eq!(found[2].color, 0xFF_763968);
        assert_eq!(found[2].height, 1);
    }

    #[test]
    fn the_same_glass_twice_lengthens_the_section() {
        let found = sections(
            column(
                &[
                    BlockKind::Beacon,
                    BlockKind::RedStainedGlass,
                    BlockKind::RedStainedGlassPane,
                ],
                0,
            )
            .into_iter(),
        );
        assert_eq!(found.len(), 2);
        assert_eq!(found[1].height, 2);
    }

    #[test]
    fn an_opaque_block_ends_the_beam_without_sections() {
        let found = sections(column(&[BlockKind::Beacon, BlockKind::Stone], 3).into_iter());
        assert!(found.is_empty());
    }

    #[test]
    fn bedrock_does_not_stop_the_beam() {
        let found = sections(column(&[BlockKind::Beacon, BlockKind::Bedrock], 0).into_iter());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].height, 2);
    }

    #[test]
    fn plain_glass_lets_the_light_through_without_coloring() {
        let found = sections(column(&[BlockKind::Beacon, BlockKind::Glass], 0).into_iter());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].height, 2);
    }
}
