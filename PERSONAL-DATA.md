# Personal data

No personal data stands in this repository: not the names, handles, account ids, mail addresses, host names,
home paths, domains, devices or network details of the owner and the people they know; not a room member's
display name or id; and not a line of an actual conversation. None of them stands in code, a comment, a
document, a test fixture or a commit message.

- **A value belongs to the installation that holds it.** An address, a host, a path, a port and a credential
  are read from a settings file or the environment; the code that reads one never carries its value.
- **An example is neutral.** A person is `tester` or `qa-admin`, an agent is `agent-a` or `agent-b`, a mailbox
  stands at `example.com`, and a home path names `user`.
- **An agent is named by its registration id,** never by the name it is given. The label a person sees stands
  in the deployment's settings.
- **A test's fixtures carry no real person, agent, room or message.** A conversation line a test reads is
  written for the test.

The repository's checks hold it (`docs/checks.md`): a pattern check over every tracked file for mail addresses,
home paths, telephone numbers, tokens and private network addresses, and a names check against the list kept
outside the checkout. The third-party packs under `client/vendor/` stand as their publishers shipped them, their
own metadata included, and the pattern check passes over them.
