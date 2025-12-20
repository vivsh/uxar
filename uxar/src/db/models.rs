

pub trait IntoEntity {
    fn to_entity(&self) -> String;

    fn from_entity(entity: &str) -> Self where Self: Sized;
}


#[derive(Debug)]
pub struct TableModel{
    pub name: String,
    pub columns: Vec<ColumnModel>,
}


#[derive(Debug)]
pub struct TableDelta{
    
}


#[derive(Debug)]
pub struct ColumnModel{
    pub name: String,
    pub data_type: String,
    pub width: Option<u32>,
    pub is_nullable: bool,
}


#[derive(Debug)]
pub struct ColumnDelta{
    pub name: Option<String>,
    pub data_type: Option<String>,
    pub width: Option<Option<u32>>,
    pub is_nullable: Option<bool>,   
}

pub struct QualifiedName{
    pub schema: Option<String>,
    pub name: String,
}

#[derive(Debug)]
pub enum ColumnPatch {
    Added{path: String, column: ColumnModel},
    Removed{path: String, schema: Option<String>},
    Modified{path: String, delta: ColumnDelta},
}