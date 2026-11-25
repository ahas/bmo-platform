//! What the work-order runner writes as a stop reason of its own (docs/work-orders.md).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkOrderStops {
  pub final_not_closed: String,
}

crate::part!(pub WORK_ORDER_STOPS: WorkOrderStops = "work-order-stops");
