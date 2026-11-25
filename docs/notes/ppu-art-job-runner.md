---
id: ppu-art-job-runner
kind: recipe
title: ppu art job runs staged job files, so a plugin with no shell calls ppu with one fixed command line
codes: []
---
Windows has no shell for a plugin to lean on, and `timeout` and `&` are not portable, so the plugin stages a
job in a folder and starts `ppu art job <folder>`. A job is `<name>.args` (one `ppu` argument per line, so
the lines start with `art`, `check`, ...) and optionally `<name>.in`, its stdin. Write `.in` first and
`.args` last: the runner takes a job when `.args` appears. It claims it by renaming to `<name>.taken` (a
second runner skips it, so a job never runs twice), writes `<name>.out` and `<name>.err` whole, then
`<name>.code` last: once `.code` exists the other two are complete. After `--timeout` seconds (default 20)
a job is killed with code 124. `--detach` (leave it out to run the staged jobs once and exit) starts the runner in the background and returns at once; the
plugin polls for `.code`, reads the three files and deletes them.

```sh
mkdir .ppu-jobs
printf 'art\ncheck\nassets/sprites.png\n' > .ppu-jobs/one.args
ppu art job .ppu-jobs
cat .ppu-jobs/one.code
```
