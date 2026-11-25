// Agency's prompt-composing entry (docs/agent-documents.md, 「How the documents are composed」): what every
// agent of agency's is given — composed prompts and runner directories — written under the agents directory
// the data root holds, and listed. No gate's prompt is written: a gate hands its own with each turn. The
// server runs it on every start, the reload path whenever an agent document changes, and a person by hand to
// see or refresh what is written.
import { agentFaultLine, deployAgents, deployRoot } from "./agents";

const deployment = await deployAgents();
const count = (value: number): string => value.toLocaleString("en-US");

console.log(`deployed ${count(deployment.files.length)} files under ${deployRoot()}`);

for (const file of deployment.files) {
  console.log(
    `  ${count(file.bytes).padStart(9)} B  ~${count(Math.round(file.bytes / 2.5)).padStart(7)} tok  ${file.path.slice(deployRoot().length + 1)}`,
  );
}

for (const fault of deployment.faults) {
  console.error(agentFaultLine(fault));
}
