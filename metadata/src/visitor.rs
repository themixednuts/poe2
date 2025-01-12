pub trait Visitor {
    fn visit_version(&mut self, version: u32);
    fn visit_extends(&mut self, extended_by: Option<&str>);
    fn visit_abstract(&mut self, is_abstract: bool);
}
