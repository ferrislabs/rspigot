#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamedColor {
    Black,
    DarkBlue,
    DarkGreen,
    DarkAqua,
    DarkRed,
    DarkPurple,
    Gold,
    Gray,
    DarkGray,
    Blue,
    Green,
    Aqua,
    Red,
    LightPurple,
    Yellow,
    White,
}

impl NamedColor {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Black => "black",
            Self::DarkBlue => "dark_blue",
            Self::DarkGreen => "dark_green",
            Self::DarkAqua => "dark_aqua",
            Self::DarkRed => "dark_red",
            Self::DarkPurple => "dark_purple",
            Self::Gold => "gold",
            Self::Gray => "gray",
            Self::DarkGray => "dark_gray",
            Self::Blue => "blue",
            Self::Green => "green",
            Self::Aqua => "aqua",
            Self::Red => "red",
            Self::LightPurple => "light_purple",
            Self::Yellow => "yellow",
            Self::White => "white",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        NAMED_COLORS.into_iter().find(|color| color.name() == name)
    }

    pub fn from_legacy_code(code: char) -> Option<Self> {
        let index = usize::try_from(code.to_digit(16)?).ok()?;
        NAMED_COLORS.get(index).copied()
    }

    pub fn legacy_code(self) -> char {
        char::from(LEGACY_CODES[self as usize])
    }
}

const NAMED_COLORS: [NamedColor; 16] = [
    NamedColor::Black,
    NamedColor::DarkBlue,
    NamedColor::DarkGreen,
    NamedColor::DarkAqua,
    NamedColor::DarkRed,
    NamedColor::DarkPurple,
    NamedColor::Gold,
    NamedColor::Gray,
    NamedColor::DarkGray,
    NamedColor::Blue,
    NamedColor::Green,
    NamedColor::Aqua,
    NamedColor::Red,
    NamedColor::LightPurple,
    NamedColor::Yellow,
    NamedColor::White,
];
const LEGACY_CODES: &[u8; 16] = b"0123456789abcdef";
const FLAG_CODES: [char; 5] = ['l', 'o', 'n', 'm', 'k'];
const SECTION: char = '§';
const RGB_MAX: u32 = 0xFF_FFFF;
const SPIGOT_RESET: Style = Style {
    click: None,
    hover: None,
    extras: None,
    color: None,
    bold: Some(false),
    italic: Some(false),
    underlined: Some(false),
    strikethrough: Some(false),
    obfuscated: Some(false),
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    Named(NamedColor),
    Rgb(u32),
}

impl Color {
    pub fn parse(name: &str) -> Option<Self> {
        match name.strip_prefix('#') {
            Some(hex) => u32::from_str_radix(hex, 16)
                .ok()
                .filter(|rgb| *rgb <= RGB_MAX)
                .map(Self::Rgb),
            None => NamedColor::from_name(name).map(Self::Named),
        }
    }

    pub fn serialize(self) -> String {
        match self {
            Self::Named(named) => named.name().to_owned(),
            Self::Rgb(rgb) => format!("#{:06X}", rgb & RGB_MAX),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickEvent {
    RunCommand(String),
    SuggestCommand(String),
    OpenUrl(String),
    OpenFile(String),
    ChangePage(i32),
    CopyToClipboard(String),
    ShowDialog(String),
    Custom(String),
    CustomPayload { id: String, payload: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoverTarget {
    Item {
        id: String,
        count: i32,
        components: String,
    },
    Entity {
        id: String,
        uuid: [i32; 4],
        name: Option<Box<Component>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StyleExtras {
    pub hover_target: Option<HoverTarget>,
    pub insertion: Option<Box<str>>,
    pub font: Option<Box<str>>,
    pub shadow_color: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Style {
    pub click: Option<Box<ClickEvent>>,
    pub hover: Option<Box<Component>>,
    pub extras: Option<Box<StyleExtras>>,
    pub color: Option<Color>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underlined: Option<bool>,
    pub strikethrough: Option<bool>,
    pub obfuscated: Option<bool>,
}

impl Style {
    pub fn insertion(&self) -> Option<&str> {
        self.extras.as_deref()?.insertion.as_deref()
    }

    pub fn font(&self) -> Option<&str> {
        self.extras.as_deref()?.font.as_deref()
    }

    pub fn shadow_color(&self) -> Option<u32> {
        self.extras.as_deref()?.shadow_color
    }

    pub fn hover_target(&self) -> Option<&HoverTarget> {
        self.extras.as_deref()?.hover_target.as_ref()
    }

    pub fn with_extras(&mut self, change: impl FnOnce(&mut StyleExtras)) {
        let mut extras = self.extras.take().map(|boxed| *boxed).unwrap_or_default();
        change(&mut extras);
        self.extras = (extras != StyleExtras::default()).then(|| Box::new(extras));
    }

    pub(crate) const fn flags(&self) -> [Option<bool>; 5] {
        [
            self.bold,
            self.italic,
            self.underlined,
            self.strikethrough,
            self.obfuscated,
        ]
    }

    pub(crate) const fn from_parts(
        color: Option<Color>,
        [bold, italic, underlined, strikethrough, obfuscated]: [Option<bool>; 5],
    ) -> Self {
        Self {
            click: None,
            hover: None,
            extras: None,
            color,
            bold,
            italic,
            underlined,
            strikethrough,
            obfuscated,
        }
    }

    fn inherit(&self, parent: &Self) -> Self {
        let mut flags = self.flags();
        for (flag, inherited) in flags.iter_mut().zip(parent.flags()) {
            *flag = flag.or(inherited);
        }
        Self {
            click: self.click.clone().or_else(|| parent.click.clone()),
            hover: self.hover.clone().or_else(|| parent.hover.clone()),
            extras: match (&self.extras, &parent.extras) {
                (Some(own), Some(inherited)) => Some(Box::new(StyleExtras {
                    hover_target: own
                        .hover_target
                        .clone()
                        .or_else(|| inherited.hover_target.clone()),
                    insertion: own
                        .insertion
                        .clone()
                        .or_else(|| inherited.insertion.clone()),
                    font: own.font.clone().or_else(|| inherited.font.clone()),
                    shadow_color: own.shadow_color.or(inherited.shadow_color),
                })),
                (Some(own), None) => Some(own.clone()),
                (None, inherited) => inherited.clone(),
            },
            ..Self::from_parts(self.color.or(parent.color), flags)
        }
    }

    fn write_change(&self, next: &Self, out: &mut String) {
        let adds_flags = self.color == next.color
            && self
                .flags()
                .into_iter()
                .zip(next.flags())
                .all(|(was, is)| was == is || is == Some(true));
        let kept = if adds_flags {
            self.flags()
        } else {
            match next.color {
                Some(Color::Named(color)) => {
                    out.push(SECTION);
                    out.push(color.legacy_code());
                }
                Some(Color::Rgb(_)) => {}
                None => {
                    out.push(SECTION);
                    out.push('r');
                }
            }
            [None; 5]
        };
        for ((kept, is), code) in kept.into_iter().zip(next.flags()).zip(FLAG_CODES) {
            if is == Some(true) && kept != Some(true) {
                out.push(SECTION);
                out.push(code);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    Text(String),
    Translatable {
        key: String,
        fallback: Option<Box<String>>,
        args: Vec<Component>,
    },
    Score {
        name: String,
        objective: String,
    },
    Selector {
        pattern: String,
        separator: Option<Box<Component>>,
    },
    Keybind(String),
    Nbt(Box<NbtContents>),
    Object(Box<ObjectNode>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NbtContents {
    pub path: String,
    pub interpret: bool,
    pub plain: bool,
    pub separator: Option<Box<Component>>,
    pub source: NbtSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectNode {
    pub object: ObjectContents,
    pub fallback: Option<Box<Component>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NbtSource {
    Block(String),
    Entity(String),
    Storage(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectContents {
    Atlas { atlas: String, sprite: String },
    Player { profile: String, hat: bool },
}

impl Content {
    pub fn static_text(&self) -> &str {
        match self {
            Self::Text(text) => text,
            Self::Translatable { key, .. } => key,
            Self::Selector { pattern, .. } => pattern,
            Self::Keybind(key) => key,
            Self::Score { .. } | Self::Nbt(_) | Self::Object(_) => "",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Component {
    pub content: Content,
    pub style: Style,
    pub extra: Vec<Component>,
}

impl Component {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: Content::Text(text.into()),
            style: Style::default(),
            extra: Vec::new(),
        }
    }

    pub fn translatable(key: impl Into<String>, args: Vec<Component>) -> Self {
        Self {
            content: Content::Translatable {
                key: key.into(),
                fallback: None,
                args,
            },
            style: Style::default(),
            extra: Vec::new(),
        }
    }

    pub fn color(mut self, color: NamedColor) -> Self {
        self.style.color = Some(Color::Named(color));
        self
    }

    pub fn bold(mut self) -> Self {
        self.style.bold = Some(true);
        self
    }

    pub fn italic(mut self, value: bool) -> Self {
        self.style.italic = Some(value);
        self
    }

    pub fn underlined(mut self, value: bool) -> Self {
        self.style.underlined = Some(value);
        self
    }

    pub fn append(mut self, child: Component) -> Self {
        self.extra.push(child);
        self
    }

    pub fn from_legacy(input: &str) -> Self {
        parse_legacy(input, |c| c == '§' || c == '&', &Style::default())
    }

    pub fn from_section_codes(input: &str) -> Self {
        parse_legacy(input, |c| c == '§', &SPIGOT_RESET)
    }

    pub fn to_legacy(&self) -> String {
        let mut out = String::new();
        let mut last = Style::default();
        self.write_legacy(&Style::default(), &mut last, &mut out);
        out
    }

    fn write_legacy(&self, parent: &Style, last: &mut Style, out: &mut String) {
        let style = self.style.inherit(parent);
        let content = self.content.static_text();
        if !content.is_empty() {
            if style != *last {
                last.write_change(&style, out);
                *last = style.clone();
            }
            out.push_str(content);
        }
        for child in &self.extra {
            child.write_legacy(&style, last, out);
        }
    }

    pub fn render(&self, translate: &impl Fn(&str) -> Option<&'static str>) -> String {
        let mut out = String::new();
        self.write_rendered(&mut out, translate);
        out
    }

    fn write_rendered(&self, out: &mut String, translate: &impl Fn(&str) -> Option<&'static str>) {
        match &self.content {
            Content::Translatable {
                key,
                fallback,
                args,
            } => match translate(key) {
                Some(template) => out.push_str(&fill_template(template, args, translate)),
                None if fallback.is_some() => out.push_str(&fill_template(
                    fallback.as_deref().map_or("", String::as_str),
                    args,
                    translate,
                )),
                None => {
                    out.push_str(key);
                    for arg in args {
                        out.push(' ');
                        arg.write_rendered(out, translate);
                    }
                }
            },
            other => out.push_str(other.static_text()),
        }
        for child in &self.extra {
            child.write_rendered(out, translate);
        }
    }

    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        self.write_plain(&mut out);
        out
    }

    fn write_plain(&self, out: &mut String) {
        match &self.content {
            Content::Translatable { key, args, .. } => {
                out.push_str(key);
                for arg in args {
                    out.push(' ');
                    arg.write_plain(out);
                }
            }
            other => out.push_str(other.static_text()),
        }
        for child in &self.extra {
            child.write_plain(out);
        }
    }
}

fn parse_legacy(input: &str, is_marker: impl Fn(char) -> bool, reset: &Style) -> Component {
    let mut root = Component::text("");
    let mut current = Style::default();
    let mut buffer = String::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if is_marker(ch) && chars.peek().is_some_and(|c| is_legacy_code(*c)) {
            let code = chars.next().unwrap_or('r');
            if !buffer.is_empty() {
                root.extra.push(Component {
                    content: Content::Text(std::mem::take(&mut buffer)),
                    style: current.clone(),
                    extra: Vec::new(),
                });
            }
            apply_legacy(&mut current, code, reset);
        } else {
            buffer.push(ch);
        }
    }
    if !buffer.is_empty() {
        root.extra.push(Component {
            content: Content::Text(buffer),
            style: current,
            extra: Vec::new(),
        });
    }
    if root.extra.len() == 1 {
        return root.extra.remove(0);
    }
    root
}

fn fill_template(
    template: &str,
    args: &[Component],
    translate: &impl Fn(&str) -> Option<&'static str>,
) -> String {
    let mut out = String::with_capacity(template.len());
    let mut next = 0usize;
    let mut rest = template;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(tail) = after.strip_prefix('s') {
            if let Some(arg) = args.get(next) {
                arg.write_rendered(&mut out, translate);
            }
            next += 1;
            rest = tail;
        } else if let Some((index, tail)) = after
            .split_once("$s")
            .and_then(|(n, t)| n.parse::<usize>().ok().map(|n| (n, t)))
        {
            if let Some(arg) = index.checked_sub(1).and_then(|i| args.get(i)) {
                arg.write_rendered(&mut out, translate);
            }
            rest = tail;
        } else if let Some(tail) = after.strip_prefix('%') {
            out.push('%');
            rest = tail;
        } else {
            out.push('%');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn is_legacy_code(c: char) -> bool {
    matches!(c.to_ascii_lowercase(), '0'..='9' | 'a'..='f' | 'k'..='o' | 'r')
}

fn apply_legacy(style: &mut Style, code: char, reset: &Style) {
    if let Some(color) = NamedColor::from_legacy_code(code) {
        *style = Style {
            color: Some(Color::Named(color)),
            ..reset.clone()
        };
        return;
    }
    match code.to_ascii_lowercase() {
        'k' => style.obfuscated = Some(true),
        'l' => style.bold = Some(true),
        'm' => style.strikethrough = Some(true),
        'n' => style.underlined = Some(true),
        'o' => style.italic = Some(true),
        _ => *style = reset.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_fills_sequential_and_positional_arguments() {
        let lookup = |key: &str| match key {
            "a" => Some("%s joined %s"),
            "b" => Some("%2$s then %1$s, 100%%"),
            _ => None,
        };
        let c = Component::translatable("a", vec![Component::text("Alice"), Component::text("x")]);
        assert_eq!(c.render(&lookup), "Alice joined x");
        let c = Component::translatable("b", vec![Component::text("one"), Component::text("two")]);
        assert_eq!(c.render(&lookup), "two then one, 100%");
        assert_eq!(
            Component::translatable("missing", Vec::new()).render(&lookup),
            "missing"
        );
    }

    #[test]
    fn legacy_codes_split_into_styled_parts() {
        let c = Component::from_legacy("&aHello §lWorld");
        assert_eq!(c.plain_text(), "Hello World");
        assert_eq!(c.extra.len(), 2);
        assert_eq!(
            c.extra[0].style.color,
            Some(Color::Named(NamedColor::Green))
        );
        assert_eq!(c.extra[1].style.bold, Some(true));
    }

    #[test]
    fn section_codes_ignore_ampersands() {
        let c = Component::from_section_codes("R&D &aok §6Gold §lnow");
        assert_eq!(c.plain_text(), "R&D &aok Gold now");
        assert_eq!(c.extra.len(), 3);
        assert_eq!(c.extra[0].style, Style::default());
        assert_eq!(c.extra[1].style.color, Some(Color::Named(NamedColor::Gold)));
        assert_eq!(c.extra[2].style.bold, Some(true));
        assert_eq!(c.extra[2].style.color, Some(Color::Named(NamedColor::Gold)));

        let plain = Component::from_section_codes("R&D &aok");
        assert_eq!(plain, Component::text("R&D &aok"));
        assert_eq!(Component::from_section_codes(""), Component::text(""));
    }

    #[test]
    fn to_legacy_round_trips_section_strings() {
        for s in [
            "plain",
            "§6Excalibur",
            "§6§lBold gold",
            "§6Gold §lbold",
            "§lA§6B",
            "§6A§rB",
        ] {
            assert_eq!(Component::from_section_codes(s).to_legacy(), s);
        }
    }

    #[test]
    fn section_codes_reset_like_spigot() {
        let spigot_reset = Style {
            click: None,
            hover: None,
            extras: None,
            color: None,
            bold: Some(false),
            italic: Some(false),
            underlined: Some(false),
            strikethrough: Some(false),
            obfuscated: Some(false),
        };
        let plain = Component::from_section_codes("§rPlain");
        assert_eq!(plain.content, Content::Text("Plain".to_owned()));
        assert_eq!(plain.style, spigot_reset);
        let gold = Component::from_section_codes("§6Gold");
        assert_eq!(
            gold.style,
            Style {
                color: Some(Color::Named(NamedColor::Gold)),
                ..spigot_reset.clone()
            }
        );
        let bold = Component::from_section_codes("§r§lBold");
        assert_eq!(
            bold.style,
            Style {
                bold: Some(true),
                ..spigot_reset
            }
        );
        assert_eq!(Component::from_legacy("§rPlain"), Component::text("Plain"));
        assert_eq!(
            Component::from_legacy("&6Gold").style,
            Style {
                color: Some(Color::Named(NamedColor::Gold)),
                ..Style::default()
            }
        );
        for s in ["§rPlain", "§r§lBold", "A§rB", "§lA§rB"] {
            assert_eq!(Component::from_section_codes(s).to_legacy(), s);
        }
        let reset_then_gold = Component::from_section_codes("§r§6Gold");
        assert_eq!(reset_then_gold, gold);
        assert_eq!(
            Component::from_section_codes(&reset_then_gold.to_legacy()),
            reset_then_gold
        );
    }

    #[test]
    fn to_legacy_writes_inherited_styles_and_translation_keys() {
        let nested = Component::text("")
            .color(NamedColor::Aqua)
            .append(Component::text("A").bold())
            .append(Component::translatable("item.minecraft.stick", Vec::new()));
        assert_eq!(nested.to_legacy(), "§b§lA§bitem.minecraft.stick");
        let mut rgb = Component::text("C");
        rgb.style.color = Some(Color::Rgb(0x123456));
        rgb.style.italic = Some(true);
        assert_eq!(rgb.to_legacy(), "§oC");
    }

    #[test]
    fn named_colors_keep_their_name_and_legacy_code() {
        for code in ('0'..='9').chain('a'..='f') {
            let color = NamedColor::from_legacy_code(code).unwrap();
            assert_eq!(color.legacy_code(), code);
            assert_eq!(NamedColor::from_name(color.name()), Some(color));
            assert_eq!(Color::parse(color.name()), Some(Color::Named(color)));
        }
        assert_eq!(Color::parse("#00FF7f"), Some(Color::Rgb(0x00FF7F)));
        assert_eq!(Color::Rgb(0x00FF7F).serialize(), "#00FF7F");
        assert_eq!(Color::Named(NamedColor::Gold).serialize(), "gold");
        assert_eq!(Color::parse("#1000000"), None);
        assert_eq!(Color::parse("#-1"), None);
        assert_eq!(Color::parse("golden"), None);
    }
}
