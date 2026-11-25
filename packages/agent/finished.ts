// A run that orchestrates work of its own can end while that work is still going, and what the
// process then hands back is a note about the work rather than the work's result. The work does
// not survive it either: whatever the run left running is killed when the run ends, so the note
// promises something that can never arrive and whoever was waiting waits forever.
//
// Whether a run finished is therefore decided out here, by the process that started it, and
// never by the run's own account of itself — that account is the thing in question, and a rule
// in a prompt is advice the run can reason its way around. What the run left running is the
// test that settles it, because it is a fact about the process rather than about the words:
// nothing the run can say makes a task it abandoned finished. The shape test below is the
// second net, for a run that narrated its plan without leaving anything behind.

// A background task of the run's own, as its output stream names it.
export interface LiveTask {
  id: string;
  description: string;
}

// A workflow the run launched, as its output stream names it. A workflow is not a chain of tool
// calls the stream carries — the CLI runs it as a background task of its own — so what is known
// about it is the call it was launched under, what it was launched as, and the word its ending
// carried, if an ending was reported at all.
export interface RunWorkflow {
  call: string;
  name: string;
  status: string | null;
}

