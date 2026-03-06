#[cfg(test)]
mod tests {
    use super::super::*;
    use schemars::JsonSchema;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;

    // Test types
    #[derive(Debug, Clone, JsonSchema, serde::Serialize, serde::Deserialize)]
    struct TestPayload {
        value: i32,
    }

    #[derive(Debug, Clone, JsonSchema, serde::Serialize, serde::Deserialize)]
    struct TestResponse {
        result: String,
    }

    #[derive(Debug, Clone)]
    struct TestContext {
        id: u64,
    }

    impl IntoPayloadData for TestContext {
        fn into_payload_data(self) -> PayloadData {
            PayloadData::new(TestPayload { value: 42 })
        }
    }

    #[derive(Debug, Clone, JsonSchema)]
    struct TestExtractor(String);

    impl IntoArgPart for TestExtractor {
        fn into_arg_part() -> ArgPart {
            ArgPart::Query(TypeSchema::wrap::<String>())
        }
    }

    impl FromContextParts<TestContext> for TestExtractor {
        fn from_context_parts(_ctx: &TestContext) -> Result<Self, CallError> {
            Ok(TestExtractor("test".to_string()))
        }
    }

    impl FromContext<TestContext> for TestExtractor {
        fn from_context(ctx: TestContext) -> Result<Self, CallError> {
            Self::from_context_parts(&ctx)
        }
    }

    impl IntoReturnPart for TestResponse {
        fn into_return_part() -> ReturnPart {
            ReturnPart::Body(TypeSchema::wrap::<TestResponse>(), "application/json".into())
        }
    }

    // Test handlers with different arities
    fn handler_no_args() -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async {})
    }

    fn handler_one_arg(
        _ex: TestExtractor,
    ) -> Pin<Box<dyn Future<Output = Payload<TestResponse>> + Send>> {
        Box::pin(async { Payload::from(TestResponse { result: "ok".into() }) })
    }

    fn handler_with_payload(
        _payload: Payload<TestPayload>,
    ) -> Pin<Box<dyn Future<Output = Payload<TestResponse>> + Send>> {
        Box::pin(async { Payload::from(TestResponse { result: "ok".into() }) })
    }

    fn handler_two_args(
        _ex1: TestExtractor,
        _payload: Payload<TestPayload>,
    ) -> Pin<Box<dyn Future<Output = Payload<TestResponse>> + Send>> {
        Box::pin(async { Payload::from(TestResponse { result: "ok".into() }) })
    }

    #[tokio::test]
    async fn test_callable_no_args() {
        let callable = Callable::<TestContext, CallError>::new(handler_no_args);
        
        let ctx = TestContext { id: 1 };
        let result = callable.call(ctx).await;
        
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_callable_one_arg() {
        let callable = Callable::<TestContext, CallError>::new(handler_one_arg);
        
        let spec = callable.inspect();
        assert_eq!(spec.arity(), 1);
        assert!(matches!(spec.args[0].part, ArgPart::Query(_)));
        
        let ctx = TestContext { id: 1 };
        let result = callable.call(ctx).await;
        
        assert!(result.is_ok());
        let payload = result.unwrap();
        let response = payload.downcast_ref::<TestResponse>();
        assert!(response.is_some());
        assert_eq!(response.unwrap().result, "ok");
    }

    #[tokio::test]
    async fn test_callable_with_payload() {
        let callable = Callable::<TestContext, CallError>::new(handler_with_payload);
        
        let spec = callable.inspect();
        assert_eq!(spec.arity(), 1);
        assert!(matches!(spec.args[0].part, ArgPart::Body(_, _)));
        
        // Check payload type tracking
        let payload_type = spec.payload_type();
        assert!(payload_type.is_some());
        assert_eq!(payload_type.unwrap(), std::any::TypeId::of::<TestPayload>());
        
        let ctx = TestContext { id: 1 };
        let result = callable.call(ctx).await;
        
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_callable_two_args() {
        let callable = Callable::<TestContext, CallError>::new(handler_two_args);
        
        let spec = callable.inspect();
        assert_eq!(spec.arity(), 2);
        assert!(matches!(spec.args[0].part, ArgPart::Query(_)));
        assert!(matches!(spec.args[1].part, ArgPart::Body(_, _)));
        
        let ctx = TestContext { id: 1 };
        let result = callable.call(ctx).await;
        
        assert!(result.is_ok());
    }

    #[test]
    fn test_callable_clone() {
        let callable = Callable::<TestContext, CallError>::new(handler_no_args);
        let cloned = callable.clone();
        
        assert_eq!(callable.inspect().name, cloned.inspect().name);
        assert_eq!(callable.inspect().arity(), cloned.inspect().arity());
    }

    #[test]
    fn test_call_spec_introspection() {
        let callable = Callable::<TestContext, CallError>::new(handler_two_args);
        let spec = callable.inspect();
        
        // Check basic metadata
        assert!(!spec.name.is_empty());
        assert!(!spec.is_method);
        assert!(spec.receiver.is_none());
        
        // Check arguments
        assert_eq!(spec.args.len(), 2);
        assert_eq!(spec.args[0].position, 0);
        assert_eq!(spec.args[1].position, 1);
        assert!(spec.args[0].name.starts_with("arg"));
        
        // Check returns
        assert_eq!(spec.returns.len(), 1);
        assert!(matches!(spec.returns[0].part, ReturnPart::Body(_, _)));
    }

    #[tokio::test]
    async fn test_payload_extraction() {
        let callable = Callable::<TestContext, CallError>::new(handler_with_payload);
        
        let ctx = TestContext { id: 1 };
        let result = callable.call(ctx).await;
        
        assert!(result.is_ok());
        let data = result.unwrap();
        
        // Test downcast_ref
        let response = data.downcast_ref::<TestResponse>();
        assert!(response.is_some());
        assert_eq!(response.unwrap().result, "ok");
        
        // Test wrong type
        let wrong = data.downcast_ref::<TestPayload>();
        assert!(wrong.is_none());
    }

    #[tokio::test]
    async fn test_payload_extraction_arc() {
        let callable = Callable::<TestContext, CallError>::new(handler_with_payload);
        
        let ctx = TestContext { id: 1 };
        let result = callable.call(ctx).await;
        
        assert!(result.is_ok());
        let data = result.unwrap();
        
        // Test downcast_arc
        let response = data.into_any_arc().downcast::<TestResponse>();
        assert!(response.is_ok());
        let arc = response.unwrap();
        assert_eq!(arc.result, "ok");
    }

    #[test]
    fn test_payload_data_type_id() {
        let data = PayloadData::new(TestPayload { value: 123 });
        
        assert_eq!(data.payload_type_id(), std::any::TypeId::of::<TestPayload>());
    }

    #[test]
    fn test_deserializer_registration() {
        let callable = Callable::<TestContext, CallError>::new(handler_with_payload);
        
        // Should have deserializer since Payload<T> provides one
        let json = r#"{"value": 99}"#;
        let result = callable.deserialize(json);
        
        assert!(result.is_ok());
        let data = result.unwrap();
        let payload = data.downcast_ref::<TestPayload>();
        assert!(payload.is_some());
        assert_eq!(payload.unwrap().value, 99);
    }

    #[test]
    fn test_deserializer_not_registered() {
        let callable = Callable::<TestContext, CallError>::new(handler_one_arg);
        
        // Should not have deserializer since no Payload<T> argument
        let json = r#"{"value": 99}"#;
        let result = callable.deserialize(json);
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CallError::TypeMismatch));
    }

    #[test]
    fn test_deserializer_invalid_json() {
        let callable = Callable::<TestContext, CallError>::new(handler_with_payload);
        
        let json = r#"{"invalid": true}"#;
        let result = callable.deserialize(json);
        
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_into_output_payload() {
        let callable = Callable::<TestContext, CallError>::new(|| async {
            Payload::from(TestResponse { result: "test".into() })
        });
        
        let ctx = TestContext { id: 1 };
        let result = callable.call(ctx).await;
        
        assert!(result.is_ok());
        let data = result.unwrap();
        let response = data.downcast_ref::<TestResponse>();
        assert!(response.is_some());
        assert_eq!(response.unwrap().result, "test");
    }

    #[tokio::test]
    async fn test_into_output_unit() {
        let callable = Callable::<TestContext, CallError>::new(|| async {});
        
        let ctx = TestContext { id: 1 };
        let result = callable.call(ctx).await;
        
        assert!(result.is_ok());
        let data = result.unwrap();
        // Unit type should be stored
        let unit = data.downcast_ref::<()>();
        assert!(unit.is_some());
    }

    #[test]
    fn test_type_schema() {
        let schema = TypeSchema::wrap::<TestPayload>();
        
        assert_eq!(schema.type_id(), std::any::TypeId::of::<TestPayload>());
        
        let mut generator = schemars::SchemaGenerator::default();
        let _json_schema = schema.schema(&mut generator);
        // Schema is generated successfully
    }

    #[test]
    fn test_arg_spec_creation() {
        let spec = ArgSpec::from_type::<TestExtractor>(0, "query", "Query parameter");
        
        assert_eq!(spec.name, "query");
        assert_eq!(spec.description.as_deref(), Some("Query parameter"));
        assert_eq!(spec.position, 0);
        assert!(matches!(spec.part, ArgPart::Query(_)));
    }

    #[test]
    fn test_return_spec_creation() {
        let spec = ReturnSpec::from_type::<TestResponse>(
            Some("Success response".to_string()),
            Some(200),
        );
        
        assert_eq!(spec.description.as_deref(), Some("Success response"));
        assert_eq!(spec.status_code, Some(200));
        assert!(matches!(spec.part, ReturnPart::Body(_, _)));
    }

    #[test]
    fn test_has_payload_trait() {
        // This is a compile-time check, but we can verify the trait exists
        fn assert_has_payload<T: HasPayload<TestPayload>>() {}
        
        // These should compile
        assert_has_payload::<specs::Tuple1<Payload<TestPayload>>>();
        assert_has_payload::<specs::Tuple2<TestExtractor, Payload<TestPayload>>>();
    }

    #[test]
    fn test_into_arg_specs_empty() {
        let specs = <() as IntoArgSpecs>::into_arg_specs();
        assert_eq!(specs.len(), 0);
    }

    #[test]
    fn test_from_context_parts_unit() {
        let ctx = TestContext { id: 1 };
        let result = <() as FromContextParts<TestContext>>::from_context_parts(&ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn test_from_context_unit() {
        let ctx = TestContext { id: 1 };
        let result = <() as FromContext<TestContext>>::from_context(ctx);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_specable_no_args() {
        fn handler() -> Pin<Box<dyn Future<Output = ()> + Send>> {
            Box::pin(async {})
        }
        
        let result = Specable::call(&handler, ()).await;
        assert_eq!(result, ());
    }

    #[tokio::test]
    async fn test_specable_one_arg() {
        fn handler(_arg: TestExtractor) -> Pin<Box<dyn Future<Output = Payload<TestResponse>> + Send>> {
            Box::pin(async { Payload::from(TestResponse { result: "ok".into() }) })
        }
        
        let result = Specable::call(&handler, specs::Tuple1(TestExtractor("test".into()))).await;
        assert_eq!(result.result, "ok");
    }

    #[test]
    fn test_payload_clone() {
        let payload1 = Payload::from(TestResponse { result: "test".into() });
        let payload2 = payload1.clone();
        
        assert_eq!(payload1.result, payload2.result);
    }

    #[test]
    fn test_payload_from_arc() {
        let response = Arc::new(TestResponse { result: "test".into() });
        let payload: Payload<TestResponse> = Payload::from(response);
        
        assert_eq!(payload.result, "test");
    }

    #[test]
    fn test_call_error_variants() {
        let err1 = CallError::DeserializeFailed;
        assert!(err1.to_string().contains("deserialize"));
        
        let err2 = CallError::TypeMismatch;
        assert!(err2.to_string().contains("mismatch"));
        
        let err3 = CallError::ExtractionFailed("test".into());
        assert!(err3.to_string().contains("test"));
        
        let err4 = CallError::MissingField("field".into());
        assert!(err4.to_string().contains("field"));
        
        let err5 = CallError::InvalidArgument("arg".into());
        assert!(err5.to_string().contains("arg"));
        
        let err6 = CallError::Unauthorized;
        assert!(err6.to_string().contains("Unauthorized"));
        
        let err7 = CallError::NotFound("resource".into());
        assert!(err7.to_string().contains("resource"));
    }

    #[test]
    fn test_receiver_spec_variants() {
        assert_eq!(ReceiverSpec::Ref, ReceiverSpec::Ref);
        assert_ne!(ReceiverSpec::Ref, ReceiverSpec::MutRef);
        assert_ne!(ReceiverSpec::Value, ReceiverSpec::Box);
        assert_ne!(ReceiverSpec::Arc, ReceiverSpec::Unknown("custom"));
    }

    #[test]
    fn test_payload_data_from_arc_vs_object() {
        let response = TestResponse { result: "test".into() };
        
        let data1 = PayloadData::new(response.clone());
        let data2 = PayloadData::from_arc(Arc::new(response.clone()));
        
        assert_eq!(data1.payload_type_id(), data2.payload_type_id());
        assert_eq!(
            data1.downcast_ref::<TestResponse>().unwrap().result,
            data2.downcast_ref::<TestResponse>().unwrap().result
        );
    }

    #[tokio::test]
    async fn test_callable_execution_order() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc as StdArc;
        
        let counter = StdArc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        
        let callable = Callable::<TestContext, CallError>::new(move || {
            let counter = counter_clone.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        });
        
        let ctx1 = TestContext { id: 1 };
        let ctx2 = TestContext { id: 2 };
        
        callable.call(ctx1).await.unwrap();
        callable.call(ctx2).await.unwrap();
        
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }
}
