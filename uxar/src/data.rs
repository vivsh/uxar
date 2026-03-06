use std::{any::TypeId, collections::HashMap, sync::Arc};



pub struct DataRegistry{
    data_map: HashMap<TypeId, Arc<dyn std::any::Any + Send + Sync>>,
}

impl DataRegistry {

    pub fn new() -> Self {
        DataRegistry {
            data_map: HashMap::new(),
        }
    }

    pub fn register<T: 'static + Send + Sync>(&mut self, data: T) {
        self.data_map.insert(TypeId::of::<T>(), Arc::new(data));
    }

    pub fn get<T: 'static + Send + Sync>(&self) -> Option<Arc<T>> {
        self.data_map.get(&TypeId::of::<T>()).and_then(|data| {
            Arc::downcast::<T>(data.clone()).ok()
        })
    }

    pub fn merge(&mut self, other: DataRegistry) {
        for (key, value) in other.data_map {
            self.data_map.insert(key, value);
        }
    }

}