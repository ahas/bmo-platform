//! The harness plugin's MCP server (`docs/openapi.json`, `x-tools`): the agency's two tools, each a thin call
//! of one route through the agency's own client.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use bmo_agency_client::{
  AgencyClient, Answer, AssignWorker, Assignment, AssignmentChange, AssignmentState, ClientFailure,
  FILE_ITEM, FileItem, ItemFiled, ItemFilingAsk, Job, JobReviewAsk, JobReviewed, JobStopAsk, REVIEW_JOB,
  ReleaseWorker, ReviewJob, RunJob, STOP_JOB, StopJob,
};
use rmcp::model::{
  CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation, InitializeResult,
  JsonObject, ListToolsResult, PaginatedRequestParams, ServerCapabilities, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use schemars::generate::{SchemaGenerator, SchemaSettings};
use schemars::{JsonSchema, Schema};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

/// What the server tells a harness of itself when it starts.
const INSTRUCTIONS: &str = "Files items and runs them on the agency's workers. An item is filed with file_item, \
  which files it for the turn being carried now, and a running job is stopped on the owner's word with stop_job. \
  A harness holding an item asks for a worker \
  with assign_worker, hands the item and its lease over with run_job, and reads the job's progress, questions \
  and outcome from the job's events and fields; the harness renews its lease and reports and closes the item \
  in the job service. A failure comes back as a tool error carrying the route's status, code and sentence.";

/// What a tool's call came to short of the route's answer.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolFailure {
  /// The arguments are not the shape the tool takes.
  Arguments(String),
  /// The call reached no answer of the route's shape.
  Client(ClientFailure),
}

impl ToolFailure {
  /// The failure as the tool error's content.
  pub fn content(&self) -> Value {
    match self {
      Self::Arguments(why) => json!({ "code": "bad-request", "error": why }),
      Self::Client(ClientFailure::Answered {
        status,
        answer: Some(answer),
      }) => {
        json!({ "status": status, "code": answer.code, "error": answer.error })
      }
      Self::Client(failure @ ClientFailure::Answered { status, .. }) => {
        json!({ "status": status, "error": failure.to_string() })
      }
      Self::Client(failure) => json!({ "error": failure.to_string() }),
    }
  }
}

type Answered = Pin<Box<dyn Future<Output = Result<Value, ToolFailure>> + Send>>;
type Call = Box<dyn Fn(AgencyClient, Value) -> Answered + Send + Sync>;

/// One tool: its name, what it does, the route it calls, its input and output, and the call itself.
pub struct AgencyTool {
  pub name: &'static str,
  pub description: &'static str,
  pub route: &'static str,
  input: fn(&mut SchemaGenerator) -> Schema,
  output: fn(&mut SchemaGenerator) -> Schema,
  call: Call,
}

impl AgencyTool {
  /// The tool's input, its nested types referred to where `generator` keeps its definitions.
  pub fn input(&self, generator: &mut SchemaGenerator) -> Schema {
    (self.input)(generator)
  }

  /// The tool's output, referred to where `generator` keeps its definitions.
  pub fn output(&self, generator: &mut SchemaGenerator) -> Schema {
    (self.output)(generator)
  }

  /// Calls the tool's route on the agency `client` reaches, with the tool's arguments.
  pub fn call(&self, client: AgencyClient, arguments: Value) -> Answered {
    (self.call)(client, arguments)
  }
}

fn tool<I, O, F, Fut>(
  name: &'static str,
  description: &'static str,
  route: &'static str,
  call: F,
) -> AgencyTool
where
  I: DeserializeOwned + JsonSchema,
  O: Serialize + JsonSchema,
  F: Fn(AgencyClient, I) -> Fut + Send + Sync + 'static,
  Fut: Future<Output = Answer<O>> + Send + 'static,
{
  AgencyTool {
    name,
    description,
    route,
    input: I::json_schema,
    output: SchemaGenerator::subschema_for::<O>,
    call: Box::new(move |client, arguments| {
      let input = match serde_json::from_value::<I>(arguments) {
        Ok(input) => input,
        Err(error) => {
          let why = format!("The arguments are not the shape this tool takes: {error}.");
          return Box::pin(std::future::ready(Err(ToolFailure::Arguments(why))));
        }
      };
      let answer = call(client, input);
      Box::pin(async move {
        let answer = answer.await.map_err(ToolFailure::Client)?;
        Ok(serde_json::to_value(answer).expect("a route's answer always renders as JSON"))
      })
    }),
  }
}

/// The tools the server lists, and the document states under `x-tools`: `file_item` and `stop_job` where the
/// server was started for an agent, whose turn its items are filed and its stops asked for.
pub fn tools(agent: Option<&str>) -> Vec<AgencyTool> {
  let mut tools = vec![];

  if let Some(agent) = agent.map(str::to_owned) {
    let stopper = agent.clone();
    let reviewer = agent.clone();

    tools.push(tool::<FileItem, ItemFiled, _, _>(
      "file_item",
      FILE_ITEM,
      "POST /item-filings",
      move |client, item: FileItem| {
        let ask = ItemFilingAsk {
          agent_name: agent.clone(),
          title: item.title,
          brief: item.brief,
          queue_name: item.queue_name,
          repository: item.repository,
          priority: item.priority,
          assignee_name: item.assignee_name,
          requester_name: item.requester_name,
          labels: item.labels,
          steps: item.steps,
          steps_check: item.steps_check,
        };

        async move { client.create_item_filings(&ask).await }
      },
    ));

    tools.push(tool::<StopJob, Job, _, _>(
      "stop_job",
      STOP_JOB,
      "POST /job-stops",
      move |client, stop: StopJob| {
        let ask = JobStopAsk {
          agent_name: stopper.clone(),
          job_id: stop.job_id,
          item_id: stop.item_id,
          reason: stop.reason,
        };

        async move { client.create_job_stops(&ask).await }
      },
    ));

    tools.push(tool::<ReviewJob, JobReviewed, _, _>(
      "review_job",
      REVIEW_JOB,
      "POST /job-reviews",
      move |client, review: ReviewJob| {
        let ask = JobReviewAsk {
          agent_name: reviewer.clone(),
          job_id: review.job_id,
          action: review.action,
          text: review.text,
          stage: review.stage,
        };

        async move { client.create_job_reviews(&ask).await }
      },
    ));
  }

  tools.extend([
    tool::<AssignWorker, Assignment, _, _>(
      "assign_worker",
      "Assign a worker to an item for the harness holding it: the agency picks the worker from the item's content and the assignee it names, and holds it until the job starts or the hold runs out.",
      "POST /assignments",
      |client, body| async move { client.create_assignments(&body).await },
    ),
    tool::<ReleaseWorker, Assignment, _, _>(
      "release_worker",
      "Release an assignment the harness will not run, by its id: the worker it held is free for the next item. A hold also ends when its job ends or when the session that took it ends.",
      "PATCH /assignments/{assignmentId}",
      |client, ask: ReleaseWorker| async move {
        let change = AssignmentChange {
          state: Some(AssignmentState::Released),
        };

        client.change_assignments_by_assignment_id(ask.assignment_id, &change).await
      },
    ),
    tool::<RunJob, Job, _, _>(
      "run_job",
      "Run an item on its assigned worker: the harness hands over the item's content and the lease it holds, and the agency launches the worker's session on it; as the job ends the agency gives the lease back and queues the report in the room.",
      "POST /jobs",
      |client, body| async move { client.create_jobs(&body).await },
    ),
  ]);

  tools
}

/// The server over the agency the client reaches.
pub struct AgencyServer {
  client: AgencyClient,
  tools: Vec<AgencyTool>,
}

impl AgencyServer {
  /// The server over the agency `client` reaches, filing items for `agent`'s turns where it names one.
  pub fn new(client: AgencyClient, agent: Option<&str>) -> Self {
    Self {
      client,
      tools: tools(agent),
    }
  }

  /// The tools as MCP lists them, each input and output a schema of its own with its definitions inside.
  pub fn listed(&self) -> Vec<Tool> {
    self.tools.iter().map(listed).collect()
  }
}

fn listed(tool: &AgencyTool) -> Tool {
  let mut generator = SchemaSettings::draft2020_12()
    .for_serialize()
    .with(|settings| settings.meta_schema = None)
    .into_generator();
  let input = tool.input(&mut generator);
  let output = tool.output(&mut generator);
  let definitions = generator.take_definitions(true);
  let mut listed = Tool::new(tool.name, tool.description, standing_alone(input, &definitions));
  listed.output_schema = Some(Arc::new(standing_alone(output, &definitions)));
  listed
}

/// A schema made to stand on its own: an object at its root, with every definition it refers to beside it.
fn standing_alone(schema: Schema, definitions: &Map<String, Value>) -> JsonObject {
  let mut object = match schema.to_value() {
    Value::Object(object) if object.contains_key("$ref") => {
      let mut wrapped = Map::new();
      wrapped.insert("allOf".into(), Value::Array(vec![Value::Object(object)]));
      wrapped.insert("type".into(), "object".into());
      wrapped
    }
    Value::Object(object) => object,
    _ => unreachable!("every tool's input and output is an object"),
  };
  if !definitions.is_empty() {
    object.insert("$defs".into(), Value::Object(definitions.clone()));
  }
  object
}

impl ServerHandler for AgencyServer {
  fn get_info(&self) -> InitializeResult {
    let mut info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build());
    let mut server = Implementation::default();
    server.name = "bmo-agency".to_owned();
    server.version = crate::VERSION.to_owned();
    info.server_info = server;
    info.instructions = Some(INSTRUCTIONS.to_owned());
    info
  }

  async fn list_tools(
    &self,
    _: Option<PaginatedRequestParams>,
    _: RequestContext<RoleServer>,
  ) -> Result<ListToolsResult, ErrorData> {
    Ok(ListToolsResult::with_all_items(self.listed()))
  }

  async fn call_tool(
    &self,
    request: CallToolRequestParams,
    _: RequestContext<RoleServer>,
  ) -> Result<CallToolResponse, ErrorData> {
    let Some(tool) = self.tools.iter().find(|tool| tool.name == request.name) else {
      return Err(ErrorData::invalid_params(format!("There is no tool {}.", request.name), None));
    };
    let arguments = Value::Object(request.arguments.unwrap_or_default());
    let result = match tool.call(self.client.clone(), arguments).await {
      Ok(answer) => CallToolResult::structured(answer),
      Err(failure) => CallToolResult::error(vec![ContentBlock::text(failure.content().to_string())]),
    };
    Ok(CallToolResponse::Complete(result))
  }
}
