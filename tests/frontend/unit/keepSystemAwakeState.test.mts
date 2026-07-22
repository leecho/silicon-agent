import { readFileSync } from "node:fs";

const apiSource = readFileSync("src/api/scheduling.ts", "utf8");
const pageSource = readFileSync("src/pages/scheduling/ScheduledTasksPage.tsx", "utf8");
const schedulerCommandsSource = readFileSync("src-tauri/src/commands/scheduler.rs", "utf8");
const libSource = readFileSync("src-tauri/src/lib.rs", "utf8");

for (const fn of ["getKeepSystemAwake", "setKeepSystemAwake"]) {
  if (!apiSource.includes(`function ${fn}`)) {
    throw new Error(`scheduling api should expose ${fn}`);
  }
}

for (const command of ["get_keep_system_awake", "set_keep_system_awake"]) {
  if (!apiSource.includes(`"${command}"`)) {
    throw new Error(`scheduling api should bind ${command}`);
  }
  if (!schedulerCommandsSource.includes(`fn ${command}`)) {
    throw new Error(`scheduler commands should expose ${command}`);
  }
  if (!libSource.includes(`commands::${command}`)) {
    throw new Error(`lib.rs should register ${command}`);
  }
}

if (!pageSource.includes("getKeepSystemAwake")) {
  throw new Error("ScheduledTasksPage should read keep-awake state from backend on load");
}
if (!pageSource.includes("getKeepSystemAwake().then(setKeepAwake)")) {
  throw new Error("ScheduledTasksPage should initialize the switch from backend runtime state");
}
