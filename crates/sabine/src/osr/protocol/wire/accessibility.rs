use std::io;

use crate::osr::protocol::accessibility::{
    AxLocation, AxLocationChange, AxNode, AxTreeChange, AxTreeData, AxTreeUpdate, AxValue,
};

const STRING: u8 = 1;
const INT: u8 = 2;
const FLOAT: u8 = 3;
const BOOL: u8 = 4;
const INTS: u8 = 5;

pub(super) fn parse_tree_change(payload: &[u8]) -> io::Result<AxTreeChange> {
    let mut reader = Reader(payload);
    let tree = reader.string()?;
    let updates = reader.list(Reader::update)?;
    reader.finish()?;
    Ok(AxTreeChange { tree, updates })
}

pub(super) fn parse_location_change(payload: &[u8]) -> io::Result<AxLocationChange> {
    let mut reader = Reader(payload);
    let tree = reader.string()?;
    let locations = reader.list(|reader| Ok((reader.i32()?, reader.location()?)))?;
    reader.finish()?;
    Ok(AxLocationChange { tree, locations })
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        let (bytes, rest) = self
            .0
            .split_first_chunk::<N>()
            .ok_or_else(|| invalid("truncated accessibility tree"))?;
        self.0 = rest;
        Ok(*bytes)
    }

    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take::<1>()?[0])
    }

    fn u32(&mut self) -> io::Result<u32> {
        self.take().map(u32::from_le_bytes)
    }

    fn i32(&mut self) -> io::Result<i32> {
        self.take().map(i32::from_le_bytes)
    }

    fn f32(&mut self) -> io::Result<f32> {
        self.take().map(f32::from_le_bytes)
    }

    fn string(&mut self) -> io::Result<String> {
        let length = self.u32()? as usize;
        if length > self.0.len() {
            return Err(invalid("truncated accessibility tree"));
        }
        let (bytes, rest) = self.0.split_at(length);
        self.0 = rest;
        String::from_utf8(bytes.to_vec()).map_err(|_| invalid("accessibility text is not UTF-8"))
    }

    /// A counted list, whose count can't promise more entries than bytes left.
    fn list<T>(&mut self, mut entry: impl FnMut(&mut Self) -> io::Result<T>) -> io::Result<Vec<T>> {
        let count = self.u32()? as usize;
        if count > self.0.len() {
            return Err(invalid("truncated accessibility tree"));
        }
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            entries.push(entry(self)?);
        }
        Ok(entries)
    }

    fn finish(&self) -> io::Result<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(invalid("accessibility tree has trailing bytes"))
        }
    }

    fn update(&mut self) -> io::Result<AxTreeUpdate> {
        let data = match self.u8()? {
            0 => None,
            _ => Some(AxTreeData {
                parent_tree: self.string()?,
                focused_tree: self.string()?,
                focus: self.i32()?,
                selection_anchor: (self.i32()?, self.i32()?),
                selection_focus: (self.i32()?, self.i32()?),
            }),
        };
        Ok(AxTreeUpdate {
            data,
            clear: self.i32()?,
            root: self.i32()?,
            nodes: self.list(Self::node)?,
        })
    }

    fn node(&mut self) -> io::Result<AxNode> {
        Ok(AxNode {
            id: self.i32()?,
            role: self.string()?,
            states: self.u32()?,
            actions: self.u32()?,
            location: self.location()?,
            children: self.list(Self::i32)?,
            attributes: self.list(|reader| Ok((reader.u8()?, reader.value()?)))?,
        })
    }

    fn location(&mut self) -> io::Result<AxLocation> {
        Ok(AxLocation {
            container: self.i32()?,
            x: self.f32()?,
            y: self.f32()?,
            width: self.f32()?,
            height: self.f32()?,
            transform: match self.u8()? {
                0 => None,
                _ => Some([
                    self.f32()?,
                    self.f32()?,
                    self.f32()?,
                    self.f32()?,
                    self.f32()?,
                    self.f32()?,
                ]),
            },
        })
    }

    fn value(&mut self) -> io::Result<AxValue> {
        Ok(match self.u8()? {
            STRING => AxValue::String(self.string()?),
            INT => AxValue::Int(self.i32()?),
            FLOAT => AxValue::Float(self.f32()?),
            BOOL => AxValue::Bool(self.u8()? != 0),
            INTS => AxValue::Ints(self.list(Self::i32)?),
            _ => return Err(invalid("unknown accessibility attribute type")),
        })
    }
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Writer(Vec<u8>);

    impl Writer {
        fn u8(&mut self, value: u8) -> &mut Self {
            self.0.push(value);
            self
        }

        fn u32(&mut self, value: u32) -> &mut Self {
            self.0.extend(value.to_le_bytes());
            self
        }

        fn i32(&mut self, value: i32) -> &mut Self {
            self.0.extend(value.to_le_bytes());
            self
        }

        fn f32(&mut self, value: f32) -> &mut Self {
            self.0.extend(value.to_le_bytes());
            self
        }

        fn string(&mut self, value: &str) -> &mut Self {
            self.u32(value.len() as u32);
            self.0.extend(value.as_bytes());
            self
        }

        fn location(&mut self, container: i32, bounds: [f32; 4]) -> &mut Self {
            self.i32(container);
            for value in bounds {
                self.f32(value);
            }
            self.u8(0)
        }
    }

    fn tree_with_button() -> Vec<u8> {
        let mut writer = Writer::default();
        writer
            .string("main-tree")
            .u32(1)
            .u8(1)
            .string("")
            .string("main-tree")
            .i32(7)
            .i32(-1)
            .i32(0)
            .i32(-1)
            .i32(0)
            .i32(0)
            .i32(1)
            .u32(2);
        writer
            .i32(1)
            .string("rootWebArea")
            .u32(0)
            .u32(0)
            .location(-1, [0.0, 0.0, 800.0, 600.0])
            .u32(1)
            .i32(7)
            .u32(0);
        writer
            .i32(7)
            .string("button")
            .u32(1 << 4)
            .u32(0)
            .location(1, [10.0, 20.0, 80.0, 24.0])
            .u32(0)
            .u32(2)
            .u8(1)
            .u8(STRING)
            .string("Save")
            .u8(80)
            .u8(INTS)
            .u32(2)
            .i32(5)
            .i32(9);
        writer.0
    }

    #[test]
    fn reads_a_frame_tree() {
        let change = parse_tree_change(&tree_with_button()).unwrap();
        assert_eq!(change.tree, "main-tree");
        let update = &change.updates[0];
        assert_eq!(update.root, 1);
        assert_eq!(update.data.as_ref().unwrap().focus, 7);
        let button = &update.nodes[1];
        assert_eq!(button.role, "button");
        assert!(button.has_state(4));
        assert_eq!(button.location.container, 1);
        assert_eq!(button.location.width, 80.0);
        assert_eq!(button.string(1), Some("Save"));
        assert_eq!(button.ints(80), [5, 9]);
        assert_eq!(update.nodes[0].children, [7]);
    }

    #[test]
    fn refuses_truncated_trees() {
        let tree = tree_with_button();
        for length in [0, 4, 13, tree.len() - 1] {
            assert!(parse_tree_change(&tree[..length]).is_err());
        }
        let mut huge_count = Writer::default();
        huge_count.string("main-tree").u32(u32::MAX);
        assert!(parse_tree_change(&huge_count.0).is_err());
    }

    #[test]
    fn reads_moved_nodes() {
        let mut writer = Writer::default();
        writer
            .string("main-tree")
            .u32(1)
            .i32(7)
            .location(-1, [4.0, 8.0, 16.0, 32.0]);
        let change = parse_location_change(&writer.0).unwrap();
        assert_eq!(change.locations[0].0, 7);
        assert_eq!(change.locations[0].1.y, 8.0);
    }
}
