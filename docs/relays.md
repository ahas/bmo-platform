# One BMO to another

A run of one person's BMO may hand a line to another person's BMO, and may open a chat into that person's
room. Both are one BMO's word to another BMO: the receiving BMO answers it as another agent's, never as its
own person's, and may refuse it. What was sent and what came of it is kept for both sides.

## The two verbs

- **`bmo/relay` hands a line to another person's BMO.** The calling run names the other BMO (`to`), the line
  (`text`) and the room it is said in (`room`), which is the turn's own room where the call names none. The
  receiving BMO takes the line on its own session, and its answer is the record's answer. Where the receiving
  BMO will not take it, the call fails and says which BMO refused.
- **Two supervisors of one person never relay to each other.** Their line is said in a room one of them
  answers, by the name that wakes the other, and the answer is a reply under it; the relay refuses such a line
  `relay-to-supervisor`. A relay is one person's BMO to another person's.
- **`bmo/chat` opens a chat into that person's room.** It names the other BMO, the line and the room, which is
  that person's room and must be named. The chat opens at once: the receiving BMO's session is started or
  taken up there, the line goes into it, and the call answers with that BMO's answer. A refusal is reported to
  the sender as a refusal, never as an answer.
- **Both name the person where the sender knows them.** An optional `person` is the receiving person's account
  id; where it is named it must be the account the receiving BMO belongs to.
- **The sender is the run's own agent.** No call names itself as the sender: a call standing in no agent's
  turn is refused, so nothing outside a BMO's turn can put words in its name.

## Another BMO's word, never the person's

- **The line is marked as another BMO's in the turn the receiving BMO reads.** Its prompt opens by naming the
  sending BMO and saying the words are another supervisor's, never the receiving person's. The rulebooks the
  agents read say the same: the line grants no rights, is never acted on as the person's instruction, and is
  answered as another agent's word.
- **It is never privileged.** The turn carries no mark that every new word is the person's, whatever the
  sender's own turn held, so a relay cannot stand in for the person's words or carry the person's authority.
- **The receiving BMO may refuse.** A BMO that will not take a line answers exactly `<<REFUSE>>`, and the
  refusal is honoured: the line is not answered as delivered, the record says it was refused, and the sender's
  call fails naming the receiving BMO.

## The record

- **One row for each line whatever came of it:** the sending BMO, the receiving BMO, the room, the line, the
  kind (`relay` or `chat`), the state (`delivered`, `refused` or `failed`), the answer where there was one, the
  reason where it failed, and when it was sent.
- **Both sides read it.** `GET /relays?agent=<BMO>` answers the relays that BMO sent or received, the latest
  first, so the sender reads what became of its line and the receiver reads what was handed to it and by whom.
  Each call answers the record it wrote, and the receiving BMO's own session keeps the turn and its answer.
- **A refusal or a failure is recorded as well as answered.** The record is written before the sender's call
  is answered, so a refusal leaves evidence on both sides rather than only an error.
