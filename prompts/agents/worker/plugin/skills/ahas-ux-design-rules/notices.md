# UI Design Rules — Messages, notices and indicators

Part of the `ahas-ux-design-rules` skill, bound the same way as its `SKILL.md`, which holds the rules
every screen answers to and the sweep.

## Messages and notices

- An interface must say everything it has to say through one mechanism.
  1. There is one message component, standing in for the component library's own toaster, which is
     not stood up at all: a message sent down a second road nobody watches is lost. It tells; what
     asks is a dialog (below).
  2. The browser's own dialogs must never be used to tell or to ask: they are unstyled, outside
     every rule the interface is held to, and on a phone say nothing of where they came from.
  3. It is swept for rather than trusted: a test reads the interface's own files and fails on a
     second road.
- A transient message must stand at the top of the screen, centred, as a pill.
  1. It carries one radius at every size: a line on its own stands as a pill, and grown for a
     description or something to press it is a card of the same rounding.
  2. It is measured and given that size rather than told to fit its content, so it moves between one
     shape and the next instead of jumping.
  3. It unfolds sideways from its own centre and folds back into it, only its content fading, as
     「Motion」 in `motion.md` says of what appears from nowhere.
  4. Its surface carries its own text and mark colours, fixed whatever the theme. The tone is
     carried by the mark alone, never by the whole surface.
  5. It keeps clear of whatever the device puts at the top of its own screen, and on a narrow screen
     it stands inside the same gutter the content does.
- A notification is pressed in its whole.
  1. Wherever it leads, its title, description and mark all lead there; a button inside it is never
     the only way in.
  2. One that leads nowhere is dismissed by that same press.
  3. This holds equally for one the operating system draws and one the interface draws itself.
  4. A button inside it is only for a second thing it offers — taking back what was just done.
- Several messages arriving at once must not stack into a column, which covers the screen they are
  reporting on.
  1. One stands and the rest wait in a line.
  2. The one standing says how many are behind it.
  3. The next takes its place as the one before it goes, so the pill changes shape rather than going
     away and coming back.
  4. A message that may be said again before the first has gone is named, and the second replaces
     the first where it stands.
  5. The line is held to a length, and what is dropped is its oldest end.
- A question the user must answer before an action — a yes-or-no confirmation included — stands in
  a dialog, the component library's, never in the message pill: the pill tells, it never asks.
  1. The dialog names the thing and the action it is about to take, and stands until it is answered.
  2. Its way out stands first and its way through last, at the trailing edge, and the way through
     carries the action's own verb, never a bare "yes".
  3. Closing it by the keyboard or outside it is the way out.
- A confirmation about one row or one item names that item in the dialog's own words, so the dialog
  says which row it is about; nothing about the row changes until it is answered.
- A form's own refusal stays in the form, beside the field that was refused: a state of the screen
  rather than news.
- What has not been confirmed yet is not a failure.
  1. Checking and failing are told apart by their words and by their colour.
  2. While it is still being checked, a short calm line in the quiet colour says so.
  3. Only a failure that was actually met wears the error colour, in a short line.
  4. Once it is confirmed, nothing is said.
  5. A count or a state is not shown as nought or none before it has been read.
- What the user just did is not news, and no notice restates it.
  1. A change made by the user's own press — a state toggled, an item held or released, approved,
     moved, removed, a value chosen, an entry added or saved — is shown by the thing it changed: its
     state mark, the group it stands in, the value in its field, the dialog that closed.
  2. A notice speaks for an act only where the act was refused, where it brings something the user
     has not seen yet — a new item, a place it leads to — where nothing on the screen can show it, as
     a copy to the clipboard, or where it offers to take the act back.
- Every notice goes on its own time.
  1. It closes after a few seconds set by its tone, and is never left standing until something else
     clears it.
  2. Only a notice offering the one chance to take something back, or to take up something waiting
     such as a newer version to load, has no time of its own.
  3. A line a screen keeps of its own to say what an act did is a second road and a notice that never
     goes: it is not built.

## Always-visible indicators

- An indicator that stays on screen — a status, a date, a reference time, a counter — must lead to
  what it refers to.
  1. It must respond when pressed: a persistent indicator that does nothing when pressed must not
     exist.
  2. A status leads to a page showing that status in detail, or to a modal showing it.
  3. A date or a time opens a date picker and moves to the chosen point.
  4. Any other value leads to where it is changed or seen in detail.
  5. A modal or popover opened this way must hold a button that goes to the target's own page.
  6. A pressable indicator must carry the hover and press states required of every clickable
     element.
