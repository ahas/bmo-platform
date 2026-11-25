# Asking for a secret

Read this right before asking for a secret: a key, a token, a password, a login or a file that a unit
or a setting reads by its file name under the data root's `env/`. The value is typed into the console's
secret store, never into a room; the room only ever carries the request and the saved line.

## First, look for it among the saved ones

- **A secret is asked for only where none already saved covers it.** List the names in the space the link
  would save into, never what a file holds: `ls "$BMO_DATA_DIR"/env` where you belong to no account, and
  `ls "$BMO_DATA_DIR"/env/users/<id>` with the id `BMO_OWNER_ACCOUNT` holds where you belong to one. Look for
  the name the consuming unit, setting or runbook reads, and for a name saved for the same service under
  another spelling.
- **A saved one is used by its name:** the work goes on with it, and nobody is asked to type, paste or sign in
  for it. A job that asks for a key or a sign-in is answered with the secret's name and its path under `env/`,
  which it hands to the program that reads it. A command you run on a node for the work takes it through
  `bmo/node_exec`'s own argument for saved secrets, by its name, never with the value in the command or its
  `env`.
- **Only a sign-in step the owner must take in person,** a code on his phone or a new device to approve, is
  put to him, and only that step.

## What the request names

- **The name** is the file name the consuming unit or setting reads, exactly as that reader's own
  documents or settings name it. Find it there before asking; never make one up. A name is English
  letters, digits, `.`, `_` and `-`, opens on no dot and does not end in `.previous`.
- **What goes in it** is either a registered kind, by its key, where the file the reader expects is that
  kind's shape, or the exact field keys the reader expects, each as the reader spells it, with `*` after
  a key whose value is hidden. One field is written as the bare value; several are written as
  `KEY=VALUE` lines in the order given.
- **What it is for**, in a few words in the room's language.

## Writing the link

- The console's public address is in `BMO_CONSOLE_ADDRESS` in your environment. The link is that
  address joined by one slash to `rooms/<chat>`, the chat being the one the turn's head line names, and
  then the query: `secret=<name>`, then `kind=<key>` or `fields=<KEY>,<KEY>*`, then `for=<purpose>`,
  then `user=<id>` where you belong to a person, all joined by `&`. Every value is percent-encoded, a `*`
  as `%2A`, so the chat's formatting takes nothing in the link as a mark.
- **Whose secret it is.** `BMO_OWNER_ACCOUNT` in your environment holds the user id of the person whose
  account you belong to. Where it holds one, write `user=<that id>` on every link: the secret goes into
  that person's own space, and a link without it is the platform administrator's request, which the
  person is told is not theirs. Where it is empty or unset you belong to no account: write no `user`, and
  the secret goes into the platform's own space.
- Where `BMO_CONSOLE_ADDRESS` is empty, write no link: say that the console's address is not set in
  agency's settings, and ask for no value in its place.
- Only the person the link is for can save through it: the person `user` names, else the owner. The
  console refuses everyone else. Send it where that person reads it, the room the work was asked in where
  they are in it, else their own chat.

## Waiting for it

- The console answers in the room the link names with a line in the name of the person who saved it,
  saying the secret was saved to the secret store, the secret's name and its kind in parentheses after
  it. A secret handed from the composer without a request arrives as the same kind of line.
- The work that needs the secret goes on only after such a line names the name you asked for. Until
  then it waits, and nothing stands in for the value: no guess, no placeholder, no value from
  elsewhere.
- The saved line is the confirmation. Never open the file to check it.
