use rpl_parser::pairs;

#[derive(Debug, Clone)]
pub struct Attribute {
    pub name: String,
    pub value: String,
}

impl Attribute {
    pub fn from_pairs(attr: &pairs::Attribute<'_>) -> Self {
        let (name, _, value) = attr.get_matched();
        let name = String::from(name.span.as_str());
        let value = String::from(value.span.as_str());
        Self { name, value }
    }
}
