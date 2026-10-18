import { todoPath } from "#shared/paths";
import { wording } from "#shared/wording";
import { builtConsole } from "./build";
import { clipBody } from "./push";

import type { JobQuestion, PushPayload } from "#shared/types";

// Turning the bot's news into the one line a notification carries. The bot's events say only
// that something moved, so what a notification says is read from the archive here; nothing of
// this machine goes into one — no path, no token, no agent's internals — only what the owner
// himself wrote or asked for and would recognise on a lock screen.

// The questions standing open that this console has not rung about yet. A question already
// settled is not news however lately it arrived, and one already rung about is not news twice.
export function unrungQuestions(questions: JobQuestion[], rung: ReadonlySet<number>): JobQuestion[] {
  return questions.filter((question) => question.settledAt === null && !rung.has(question.id));
}

// A notification names its item by number and title together: a number alone on a lock screen says
// nothing to somebody who does not keep every number in mind. A question whose item could not be
// read names the number alone.
export function questionPush(question: JobQuestion, ownerChatId: number | null, itemTitle: string | null = null): PushPayload {
  return {
    kind: "question",
    title: itemTitle === null
      ? wording("push.title.question", { id: question.todoId })
      : wording("push.title.question_titled", { id: question.todoId, title: itemTitle }),
    body: clipBody(question.question),
    url: todoPath(ownerChatId, question.todoId),
    // One question rings once and a repeat replaces it, so a device that was asleep wakes to
    // the question rather than to a pile of the same question.
    tag: `question:${question.id}`
  };
}



// Which console server rings, where more than one is up.
//
// The ringing is a Nitro plugin, so it is one per console server, and every console server that
// is up reads the same one stream from the bot. Two of them ring twice for the same event. That
// is not a rare shape: the deployed console is always up, and anybody working on the console has
// a second one beside it reading the same archive.
//
// A lock is this repository's own answer to two things wanting one thing — the job lock and the
// one in front of the synthesiser are both that — and it is the wrong answer here. Those
// arbitrate between equals: whoever took it first may have it and the other waits its turn.
// These two are not equals. The deployed console must ring always and the one somebody is
// working in must not, and no lock can tell which of its takers is which. First-come would let a
// development server somebody left running take the ringing away from the deployed one for as
// long as it stands, and a console that has gone quiet says nothing about having gone quiet —
// which is worse than hearing a thing twice.
//
// So it is decided by the one thing already true and different about the two, needing nothing
// said on either side: a console served from a build rings, and one served from source does not.
// The deployed unit is untouched by this and cannot be quietened by anything a developer does.
// The reading is written so that only a plain yes to the question "is this a development server"
// stops the ringing: anything else, an answer the runtime does not give included, rings.
//
// The way to watch this path ring from a development server is to say so when starting it. Both
// consoles then ring, which is the doubling on purpose; a developer who wants exactly one stops
// the deployed one first. Sending a notification to see that a device receives one at all needs
// none of this — that is what the test route is for, and it works from any server.
export function ringsFromThisServer(dev: unknown, asked: string | undefined): boolean {
  if (builtConsole(dev)) {
    return true;
  }

  const said = (asked ?? "").trim().toLowerCase();

  return said === "1" || said === "true" || said === "yes";
}

// Whether an event is worth reading the archive for at all. The stream carries a great deal
// that moves nothing a notification is about, and reading the archive for each of those would
// be work done to send nothing. A room's message comes by two words: `sent` for an answer a turn
// filed, `message` for any row written in a room.
export function mayRing(type: string): boolean {
  return type === "todo" || type === "sent" || type === "message";
}
