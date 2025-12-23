

pub trait IntoEntity {
    fn to_entity(&self) -> String;

    fn from_entity(entity: &str) -> Self where Self: Sized;
}


#[derive(Debug, Clone)]
pub struct TableModel{
    pub name: String,
    pub columns: Vec<ColumnModel>,
}


#[derive(Debug, Clone)]
pub struct TableDelta{
    
}


#[derive(Debug, Clone)]
pub struct ColumnModel{
    pub name: String,
    pub data_type: String,
    pub width: Option<u32>,
    pub is_nullable: bool,
    pub unique: bool,
    pub unique_group: Option<String>,
}


#[derive(Debug, Clone)]
pub struct ColumnDelta{
    pub name: Option<String>,
    pub data_type: Option<String>,
    pub width: Option<Option<u32>>,
    pub is_nullable: Option<bool>,   
}

#[derive(Debug, Clone)]
pub struct QualifiedName{
    pub schema: Option<String>,
    pub name: String,
}


#[derive(Debug, Clone)]
pub enum ColumnPatch {
    Added{path: String, column: ColumnModel},
    Removed{path: String, schema: Option<String>},
    Modified{path: String, delta: ColumnDelta},
}