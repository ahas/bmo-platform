// The setup every test file stands on, in the order it must run: the home of its own first, agency's
// composition beside it.
//
// `tests/packages/bunfig.toml` preloads the two for a run started from tests/packages. A test file imports this
// first as well, so that a runner starting the file from another folder gets the same setup before any
// of the file's other imports is read; the repository's suite starts every test file from the root.
// Importing a module twice in one process reads it once, so a preloaded run and an importing file
// share one setup.

import "./test-home";
import "./registrations";
