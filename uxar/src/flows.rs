use serde::{Serialize, de::DeserializeOwned};

pub enum FlowOut<F: FlowState>{
    Done(F::Output),
    Await(uuid::Uuid),
    Switch(F)
}

pub trait FlowState: Serialize + 'static + DeserializeOwned{

    type Output: Serialize + 'static;

    type Input: Serialize + 'static;

    type Error: std::fmt::Debug + 'static;

    fn validate(&self, previous: Option<&Self>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn run(self) -> Result<FlowOut<Self>, Self::Error>;

    fn start(input: Self::Input) -> Result<Self, Self::Error>;
}

