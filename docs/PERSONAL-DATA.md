# Personal data

No personal data stands in this repository: not the names, handles, account ids, mail addresses, host
names, home paths, domains, devices or other network details of the owner or of the people they know; not
a room member's display name or id; and not a line of an actual conversation. None of them stands in code,
a comment, a document, a test fixture or a commit message.

## What to write instead

- **A value belongs to the installation that holds it.** An address, a host, a path, a port and a
  credential are read from a settings file or the environment; the code that reads one never carries its
  value.
- **An example is neutral.** A person is `tester`, `qa-admin` or `someone`; a mailbox stands at
  `example.com` or another reserved example domain; a home path names `user`; a token is written in
  lowercase, where a generated secret is mixed case.
- **A test's fixtures carry no real person, account, room or message.** A conversation line a test reads is
  written for the test.

## The check

The testbed's `personal-data` check, which the declaration enables, reads every file git tracks or would
track and fails where it finds a mail address, a home path, a telephone number, a token or a private
network address. A finding names the file, the line and the kind, never the value it found. The forms are
the testbed's policy (its `docs/checks.md`, 「The quick checks」).

The check keeps the forms, not a list of names or values to look for: such a list would itself be the
personal data the rule keeps out. It admits the reserved example domains, the neutral home folders above,
a token written in lowercase as a placeholder, since a generated secret is mixed case, and a standard private
address block written whole with its prefix length, as a firewall rule names one, since such a block is
everybody's and names no one's network.

`tests/literals.rs` holds the other half: it reads this machine's account name, host name and the address
git commits under, and refuses them across the repository, the prompts among it.
