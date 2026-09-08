# Support

## Getting help

- Use GitHub Issues for bugs and concrete feature requests.
- Use Discussions, if enabled, for product questions and general usage discussion.

## Provider troubleshooting

DCC connects to the provider installations on the computer running the app.
Start with [Provider setup](README.md#provider-setup); install only the providers
you intend to use. Antigravity follows its own setup in DCC settings.

1. Verify that the provider's CLI runs in a terminal under the same OS account
   as DCC. For example, `claude --version` checks the Claude installation without
   starting a conversation.
2. Complete the provider's authentication flow. For Claude, run
   `claude auth login`; `claude auth status` checks the resulting login.
3. If the CLI is outdated or broken, update or repair it using that provider's
   installation method. DCC does not update the external provider CLIs.
4. If several installations exist, check which executable your terminal finds
   (`command -v claude` on macOS/Linux or `where.exe claude` on Windows). Shell
   aliases alone do not make a CLI available to the desktop app.
5. Return to DCC and check the provider again. Restart DCC after changing PATH
   so it can inherit the updated environment.

For provider issues, include the DCC version, CLI version, OS version and the
exact error message, along with whether the CLI works independently in a terminal.

## Before opening an issue

- Check whether the behavior is already documented.
- Include reproduction steps.
- Include platform details such as macOS or Linux version.
- Include relevant logs or screenshots when safe to share.

Do not post secrets, tokens, certificates, or private repository data in public issues.
