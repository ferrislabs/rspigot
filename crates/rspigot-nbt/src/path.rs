use crate::compound::Compound;
use crate::snbt::parse_compound;
use crate::tag::Tag;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{key}")]
pub struct PathError {
    pub key: &'static str,
}

const INVALID: PathError = PathError {
    key: "arguments.nbtpath.node.invalid",
};

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Root(Compound),
    Named(String),
    NamedFilter(String, Compound),
    All,
    Index(i32),
    Match(Compound),
}

#[derive(Debug, Clone, PartialEq)]
pub struct NbtPath {
    source: String,
    nodes: Vec<Node>,
}

fn unquoted(c: char) -> bool {
    !matches!(c, ' ' | '"' | '\'' | '[' | ']' | '.' | '{' | '}')
}

struct Reader<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Reader<'a> {
    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn bump(&mut self) {
        if let Some(c) = self.peek() {
            self.pos += c.len_utf8();
        }
    }

    fn name(&mut self) -> Result<String, PathError> {
        match self.peek() {
            Some(quote @ ('"' | '\'')) => {
                self.bump();
                let mut out = String::new();
                loop {
                    let Some(c) = self.peek() else {
                        return Err(INVALID);
                    };
                    self.bump();
                    match c {
                        '\\' => {
                            let Some(next) = self.peek() else {
                                return Err(INVALID);
                            };
                            self.bump();
                            out.push(next);
                        }
                        c if c == quote => return Ok(out),
                        c => out.push(c),
                    }
                }
            }
            _ => {
                let start = self.pos;
                while self.peek().is_some_and(unquoted) {
                    self.bump();
                }
                if start == self.pos {
                    return Err(INVALID);
                }
                Ok(self.input[start..self.pos].to_owned())
            }
        }
    }

    fn compound(&mut self) -> Result<Compound, PathError> {
        let (compound, used) = parse_compound(&self.input[self.pos..]).map_err(|_| INVALID)?;
        self.pos += used;
        Ok(compound)
    }

    fn named(&mut self) -> Result<Node, PathError> {
        let name = self.name()?;
        if self.peek() == Some('{') {
            return Ok(Node::NamedFilter(name, self.compound()?));
        }
        Ok(Node::Named(name))
    }

    fn bracket(&mut self) -> Result<Node, PathError> {
        self.bump();
        let node = match self.peek() {
            Some('{') => Node::Match(self.compound()?),
            Some(']') => Node::All,
            Some(c) if c == '-' || c.is_ascii_digit() => {
                let start = self.pos;
                self.bump();
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.bump();
                }
                let index = self.input[start..self.pos].parse().map_err(|_| INVALID)?;
                Node::Index(index)
            }
            _ => return Err(INVALID),
        };
        if self.peek() != Some(']') {
            return Err(INVALID);
        }
        self.bump();
        Ok(node)
    }
}

impl NbtPath {
    pub fn parse(input: &str) -> Result<Self, PathError> {
        let mut reader = Reader { input, pos: 0 };
        let mut nodes = Vec::new();
        if reader.peek().is_none() {
            return Err(INVALID);
        }
        if reader.peek() == Some('{') {
            nodes.push(Node::Root(reader.compound()?));
            match reader.peek() {
                None => {}
                Some('.') => reader.bump(),
                Some('[') => {}
                Some(_) => return Err(INVALID),
            }
        } else {
            nodes.push(reader.named()?);
        }
        while let Some(c) = reader.peek() {
            match c {
                '.' => {
                    reader.bump();
                    nodes.push(reader.named()?);
                }
                '[' => nodes.push(reader.bracket()?),
                _ => return Err(INVALID),
            }
        }
        Ok(Self {
            source: input.to_owned(),
            nodes,
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn get<'a>(&self, root: &'a Tag) -> Vec<&'a Tag> {
        let mut current = vec![root];
        for node in &self.nodes {
            let mut next = Vec::new();
            for tag in current {
                collect(node, tag, &mut next);
            }
            if next.is_empty() {
                return next;
            }
            current = next;
        }
        current
    }

    pub fn count(&self, root: &Tag) -> usize {
        self.get(root).len()
    }

    pub fn set(&self, root: &mut Tag, value: &Tag) -> usize {
        let Some((last, parents)) = self.nodes.split_last() else {
            return 0;
        };
        let mut targets = vec![root];
        for (offset, node) in parents.iter().enumerate() {
            let following = &self.nodes[offset + 1];
            let mut next = Vec::new();
            for tag in targets {
                descend(node, following, tag, &mut next);
            }
            if next.is_empty() {
                return 0;
            }
            targets = next;
        }
        let mut changed = 0;
        for tag in targets {
            changed += assign(last, tag, value);
        }
        changed
    }

    pub fn update(&self, root: &mut Tag, mut change: impl FnMut(&mut Tag)) -> usize {
        let mut current = vec![root];
        for node in &self.nodes {
            let mut next = Vec::new();
            for tag in current {
                collect_mut(node, tag, &mut next);
            }
            if next.is_empty() {
                return 0;
            }
            current = next;
        }
        let count = current.len();
        for tag in current {
            change(tag);
        }
        count
    }

    pub fn remove(&self, root: &mut Tag) -> usize {
        let Some((last, parents)) = self.nodes.split_last() else {
            return 0;
        };
        let mut targets = vec![root];
        for node in parents {
            let mut next = Vec::new();
            for tag in targets {
                collect_mut(node, tag, &mut next);
            }
            if next.is_empty() {
                return 0;
            }
            targets = next;
        }
        targets.into_iter().map(|tag| erase(last, tag)).sum()
    }
}

fn elements(tag: &Tag) -> Option<Vec<&Tag>> {
    match tag {
        Tag::List(list) => Some(list.iter().collect()),
        _ => None,
    }
}

fn collect<'a>(node: &Node, tag: &'a Tag, out: &mut Vec<&'a Tag>) {
    match node {
        Node::Root(filter) => {
            if Tag::Compound(filter.clone()).contains(tag) {
                out.push(tag);
            }
        }
        Node::Named(name) => {
            if let Some(child) = tag.as_compound().and_then(|c| c.get(name)) {
                out.push(child);
            }
        }
        Node::NamedFilter(name, filter) => {
            if let Some(child) = tag.as_compound().and_then(|c| c.get(name))
                && Tag::Compound(filter.clone()).contains(child)
            {
                out.push(child);
            }
        }
        Node::All => out.extend(elements(tag).unwrap_or_default()),
        Node::Index(index) => {
            if let Some(list) = elements(tag)
                && let Some(found) = resolve(*index, list.len()).and_then(|i| list.get(i))
            {
                out.push(found);
            }
        }
        Node::Match(filter) => {
            let wanted = Tag::Compound(filter.clone());
            out.extend(
                elements(tag)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|element| wanted.contains(element)),
            );
        }
    }
}

fn resolve(index: i32, len: usize) -> Option<usize> {
    let resolved = if index < 0 {
        len as i64 + i64::from(index)
    } else {
        i64::from(index)
    };
    usize::try_from(resolved).ok().filter(|i| *i < len)
}

fn collect_mut<'a>(node: &Node, tag: &'a mut Tag, out: &mut Vec<&'a mut Tag>) {
    match node {
        Node::Root(filter) => {
            if Tag::Compound(filter.clone()).contains(tag) {
                out.push(tag);
            }
        }
        Node::Named(name) => {
            if let Tag::Compound(compound) = tag
                && let Some(child) = compound.get_mut(name)
            {
                out.push(child);
            }
        }
        Node::NamedFilter(name, filter) => {
            if let Tag::Compound(compound) = tag
                && let Some(child) = compound.get_mut(name)
                && Tag::Compound(filter.clone()).contains(child)
            {
                out.push(child);
            }
        }
        Node::All => {
            if let Tag::List(list) = tag {
                out.extend(list.iter_mut());
            }
        }
        Node::Index(index) => {
            if let Tag::List(list) = tag
                && let Some(i) = resolve(*index, list.len())
            {
                out.push(&mut list[i]);
            }
        }
        Node::Match(filter) => {
            if let Tag::List(list) = tag {
                let wanted = Tag::Compound(filter.clone());
                out.extend(list.iter_mut().filter(|element| wanted.contains(element)));
            }
        }
    }
}

fn blank_for(following: &Node) -> Tag {
    match following {
        Node::All | Node::Index(_) | Node::Match(_) => Tag::List(Vec::new()),
        _ => Tag::Compound(Compound::new()),
    }
}

fn descend<'a>(node: &Node, following: &Node, tag: &'a mut Tag, out: &mut Vec<&'a mut Tag>) {
    match node {
        Node::Named(name) => {
            if let Tag::Compound(compound) = tag {
                if !compound.contains_key(name) {
                    compound.insert(name.clone(), blank_for(following));
                }
                if let Some(child) = compound.get_mut(name) {
                    out.push(child);
                }
            }
        }
        Node::NamedFilter(name, filter) => {
            if let Tag::Compound(compound) = tag {
                if !compound.contains_key(name) {
                    compound.insert(name.clone(), Tag::Compound(filter.clone()));
                }
                if let Some(child) = compound.get_mut(name)
                    && Tag::Compound(filter.clone()).contains(child)
                {
                    out.push(child);
                }
            }
        }
        other => collect_mut(other, tag, out),
    }
}

fn assign(node: &Node, tag: &mut Tag, value: &Tag) -> usize {
    match node {
        Node::Root(_) => 0,
        Node::Named(name) => {
            let Tag::Compound(compound) = tag else {
                return 0;
            };
            if compound.get(name) == Some(value) {
                return 0;
            }
            compound.insert(name.clone(), value.clone());
            1
        }
        Node::NamedFilter(name, filter) => {
            let Tag::Compound(compound) = tag else {
                return 0;
            };
            let current = compound.get(name);
            let matches =
                current.is_some_and(|child| Tag::Compound(filter.clone()).contains(child));
            if current.is_some() && !matches || current == Some(value) {
                return 0;
            }
            compound.insert(name.clone(), value.clone());
            1
        }
        Node::All => {
            let Tag::List(list) = tag else {
                return 0;
            };
            let mut changed = 0;
            for element in list.iter_mut() {
                if element != value {
                    *element = value.clone();
                    changed += 1;
                }
            }
            changed
        }
        Node::Index(index) => {
            let Tag::List(list) = tag else {
                return 0;
            };
            match resolve(*index, list.len()) {
                Some(i) if list[i] != *value => {
                    list[i] = value.clone();
                    1
                }
                _ => 0,
            }
        }
        Node::Match(filter) => {
            let Tag::List(list) = tag else {
                return 0;
            };
            let wanted = Tag::Compound(filter.clone());
            let mut changed = 0;
            for element in list.iter_mut() {
                if wanted.contains(element) && element != value {
                    *element = value.clone();
                    changed += 1;
                }
            }
            changed
        }
    }
}

fn erase(node: &Node, tag: &mut Tag) -> usize {
    match node {
        Node::Root(_) => 0,
        Node::Named(name) => match tag {
            Tag::Compound(compound) => usize::from(compound.remove(name).is_some()),
            _ => 0,
        },
        Node::NamedFilter(name, filter) => match tag {
            Tag::Compound(compound) => {
                let matches = compound
                    .get(name)
                    .is_some_and(|child| Tag::Compound(filter.clone()).contains(child));
                usize::from(matches && compound.remove(name).is_some())
            }
            _ => 0,
        },
        Node::All => match tag {
            Tag::List(list) => std::mem::take(list).len(),
            _ => 0,
        },
        Node::Index(index) => match tag {
            Tag::List(list) => match resolve(*index, list.len()) {
                Some(i) => {
                    list.remove(i);
                    1
                }
                None => 0,
            },
            _ => 0,
        },
        Node::Match(filter) => match tag {
            Tag::List(list) => {
                let wanted = Tag::Compound(filter.clone());
                let before = list.len();
                list.retain(|element| !wanted.contains(element));
                before - list.len()
            }
            _ => 0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(text: &str) -> Tag {
        Tag::Compound(parse_compound(text).expect("valid snbt").0)
    }

    fn path(text: &str) -> NbtPath {
        NbtPath::parse(text).expect("valid path")
    }

    #[test]
    fn names_indexes_and_filters_select_tags() {
        let root = tag("{a:{b:[{x:1,y:2},{x:3}],c:5},\"q r\":7}");
        assert_eq!(path("a.c").get(&root), vec![&Tag::Int(5)]);
        assert_eq!(path("a.b[1].x").get(&root), vec![&Tag::Int(3)]);
        assert_eq!(path("a.b[-1].x").get(&root), vec![&Tag::Int(3)]);
        assert_eq!(path("a.b[].x").count(&root), 2);
        assert_eq!(path("a.b[{x:3}]").count(&root), 1);
        assert_eq!(path("a{c:5}.c").count(&root), 1);
        assert_eq!(path("a{c:6}.c").count(&root), 0);
        assert_eq!(path("\"q r\"").get(&root), vec![&Tag::Int(7)]);
        assert_eq!(path("{a:{c:5}}").count(&root), 1);
        assert_eq!(path("a.missing").count(&root), 0);
    }

    #[test]
    fn malformed_paths_are_rejected() {
        for bad in ["", "a..b", "a[", "a[x]", "a.", "{", "a b"] {
            assert!(NbtPath::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn set_creates_missing_parents_and_reports_changes() {
        let mut root = tag("{a:1}");
        assert_eq!(path("b.c").set(&mut root, &Tag::Int(2)), 1);
        assert_eq!(path("b.c").get(&root), vec![&Tag::Int(2)]);
        assert_eq!(path("b.c").set(&mut root, &Tag::Int(2)), 0);
        assert_eq!(path("a").set(&mut root, &Tag::Int(9)), 1);
        let mut list = tag("{l:[1,2,3]}");
        assert_eq!(path("l[1]").set(&mut list, &Tag::Int(7)), 1);
        assert_eq!(path("l[].x").set(&mut list, &Tag::Int(7)), 0);
        assert_eq!(path("l").get(&list)[0].as_list().map(<[Tag]>::len), Some(3));
    }

    #[test]
    fn update_edits_matched_tags_in_place() {
        let mut root = tag("{l:[1,2],m:{n:[3]}}");
        let changed = path("m.n").update(&mut root, |t| {
            if let Tag::List(list) = t {
                list.push(Tag::Int(4));
            }
        });
        assert_eq!(changed, 1);
        assert_eq!(
            path("m.n").get(&root)[0].as_list().map(<[Tag]>::len),
            Some(2)
        );
        assert_eq!(path("zzz").update(&mut root, |_| {}), 0);
    }

    #[test]
    fn remove_deletes_matching_tags() {
        let mut root = tag("{a:1,l:[{x:1},{x:2},{x:1}]}");
        assert_eq!(path("a").remove(&mut root), 1);
        assert_eq!(path("l[{x:1}]").remove(&mut root), 2);
        assert_eq!(path("l").get(&root)[0].as_list().map(<[Tag]>::len), Some(1));
        assert_eq!(path("zzz").remove(&mut root), 0);
    }
}
