
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventTopic(u64);

pub enum Event<T>{
    Wait(EventTopic),
    Done(T)
}

#[derive(JsonSchema, Serialize, Deserialize, Debug, Clone)]
struct Order{
    id: i64
}

struct Payment{

}

struct Invoice{

}

struct InvoiceResult{

}

struct Shipment{

}

struct ShipmentResult{

}

struct AdminApproval{

}

fn after_order(o: Payment)->SampleFlow{
    return SampleFlow::Invoice(Invoice{}, Shipment{})
}

fn after_payment(p: Payment)->SampleFlow{
    return SampleFlow::Start(Order{id: 1})
}

fn after_invoice(i: InvoiceResult, s: ShipmentResult)->SampleFlow{
    return SampleFlow::Approval(Event::Wait(EventTopic(0)))
}

fn after_approval(a: AdminApproval)->SampleFlow{
    return SampleFlow::Done
}


#[event]
fn order_created(o: AdminApproval)->EventTopic{
    println!("Order created: {:?}", o);
    return EventTopic(0);
}

#[task]
fn handle_invoice(inv: Invoice)->InvoiceResult{
    println!("Handling invoice: {:?}", inv);
    InvoiceResult{}
}

#[task]
fn handle_shipment(ship: Shipment)->ShipmentResult{
    println!("Handling shipment: {:?}", ship);
    ShipmentResult{}
}


#[task]
fn handle_order(o: Order)->Payment{
    println!("Handling order: {:?}", o);
    Payment{}
}



pub enum SampleFlow{
    #[flow(start, then=after_order)]
    Start(Order),
    #[flow(then=after_payment)]
    Payment(Payment),
    #[flow(then=after_invoice)]
    Invoice(Invoice, Shipment),
    #[flow(then=after_approval)]
    Approval(Event<AdminApproval>),
    Done // unit variant means end of flow
}

#[flow]
fn start_payment_flow(o: Order)->SampleFlow{
    return SampleFlow::Start(o);
}