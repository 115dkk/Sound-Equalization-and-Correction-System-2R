use secs_project::PyJson;

pub(crate) trait JsonExt {
    fn get(&self, key: &str) -> Option<&PyJson>;
    fn str(&self) -> Option<&str>;
    fn object(&self) -> Option<&[(String, PyJson)]>;
    fn object_mut(&mut self) -> Option<&mut Vec<(String, PyJson)>>;
}
impl JsonExt for PyJson {
    fn get(&self, key: &str) -> Option<&PyJson> {
        self.object()?
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }
    fn str(&self) -> Option<&str> {
        if let Self::Str(s) = self {
            Some(s)
        } else {
            None
        }
    }
    fn object(&self) -> Option<&[(String, PyJson)]> {
        if let Self::Object(o) = self {
            Some(o)
        } else {
            None
        }
    }
    fn object_mut(&mut self) -> Option<&mut Vec<(String, PyJson)>> {
        if let Self::Object(o) = self {
            Some(o)
        } else {
            None
        }
    }
}
