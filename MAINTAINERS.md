# Platform maintainers

## Windows

Samuel is a candidate for the Windows maintainer role and has not been
onboarded. Once onboarded, the Windows maintainer gets GitHub Triage access
only in `pergament`. The maintainer triages Windows reports, tests portable
builds, and proposes fixes through pull requests from a fork. The project
owner merges pull requests, creates release tags, and publishes releases.

## Future Scoop bucket

The Windows maintainer gets Write access only in the separate Scoop bucket,
and only after the bucket is transferred to the project and the owner has
configured and verified protection on its default branch:

- Require pull requests for every update, including automated manifest updates.
- Require code owner approval. `CODEOWNERS` must name only the project owner
  for all paths, including the `CODEOWNERS` file itself.
- Dismiss stale approvals when new commits are pushed to a pull request.
- Give neither bots nor maintainers a bypass for these rules.

Bucket access does not change the maintainer's Triage access in `pergament`.
For each manifest update, check that the download URL points to the official
Windows portable release ZIP and that the manifest hash matches the ZIP's
SHA-256. Before adding a Scoop install link to Pergament's docs, the owner
must verify the transferred bucket and its protections. The owner must also
verify that installing and upgrading through Scoop works on Windows.

## Windows portable smoke test

1. On Windows x86_64 with the Microsoft Edge WebView2 Runtime, get the
   `Pergament-*-windows-x86_64.zip` from the `windows-x86_64-portable` artifact
   of the release workflow's manual run. For a tagged release, use the ZIP
   from the release and compare `Get-FileHash -Algorithm SHA256` for that ZIP
   with the published SHA-256 before extracting it.
2. Extract the ZIP to a fresh folder. In PowerShell, change into the extracted
   `Pergament-*-windows-x86_64` folder and run `./pergament.exe --help`. Check
   that it prints usage and exits successfully.
3. Run `./pergament-app.exe` from that folder. Check that the main window
   renders and responds. Close and reopen the app. Play a public video
   and check that both picture and sound work.
4. Report the artifact or release tested, the Windows version and build,
   and the result of each check in the release pull request or a linked issue.
   Include the steps to reproduce any failure and the exact error if shown.
