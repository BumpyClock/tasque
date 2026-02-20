import { useKeyboard, useRenderer, useTerminalDimensions } from "@opentui/react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { loadTasqueSnapshot } from "./data";
import type { BoardLane, DataSnapshot, TabId, TaskRecord } from "./types";
import {
  clampSelectionId,
  epicProgress,
  laneOrder,
  nextItemId,
  nextTab,
  pad,
  toBoardColumns,
  truncate,
} from "./view-model";

const TABS: TabId[] = ["tasks", "epics", "board"];
const REFRESH_INTERVAL_MS = 2500;

const COLORS = {
  shell: "#0D1520",
  panel: "#111B29",
  muted: "#8FA3BC",
  text: "#D7E3F4",
  active: "#58A6FF",
  warn: "#D4A65A",
  danger: "#F07178",
  ok: "#7BC77E",
};

interface BoardSelection {
  open: string | null;
  in_progress: string | null;
  done: string | null;
}

export function TasqueOpenTuiApp() {
  const renderer = useRenderer();
  const { width, height } = useTerminalDimensions();

  const [snapshot, setSnapshot] = useState<DataSnapshot>(() => loadTasqueSnapshot());
  const [activeTab, setActiveTab] = useState<TabId>("tasks");
  const [paused, setPaused] = useState(false);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [selectedEpicId, setSelectedEpicId] = useState<string | null>(null);
  const [boardLane, setBoardLane] = useState<BoardLane>("open");
  const [boardSelection, setBoardSelection] = useState<BoardSelection>({
    open: null,
    in_progress: null,
    done: null,
  });

  const tasks = snapshot.tasks;
  const epics = useMemo(() => tasks.filter((task) => task.kind === "epic"), [tasks]);
  const board = useMemo(() => toBoardColumns(tasks), [tasks]);

  const refreshNow = useCallback(() => {
    setSnapshot(loadTasqueSnapshot());
  }, []);

  useEffect(() => {
    if (paused) {
      return undefined;
    }
    const timer = setInterval(refreshNow, REFRESH_INTERVAL_MS);
    return () => clearInterval(timer);
  }, [paused, refreshNow]);

  useEffect(() => {
    setSelectedTaskId((current) => clampSelectionId(tasks, current));
  }, [tasks]);

  useEffect(() => {
    setSelectedEpicId((current) => clampSelectionId(epics, current));
  }, [epics]);

  useEffect(() => {
    setBoardSelection((current) => ({
      open: clampSelectionId(board.open, current.open),
      in_progress: clampSelectionId(board.in_progress, current.in_progress),
      done: clampSelectionId(board.done, current.done),
    }));
  }, [board.done, board.in_progress, board.open]);

  useEffect(() => {
    const lanes = laneOrder();
    if (board[boardLane].length > 0) {
      return;
    }
    const firstNonEmpty = lanes.find((lane) => board[lane].length > 0);
    if (firstNonEmpty) {
      setBoardLane(firstNonEmpty);
    }
  }, [board, boardLane]);

  const selectedBoardTaskId = boardSelection[boardLane];

  const selectedTask = useMemo(() => {
    if (activeTab === "tasks") {
      return tasks.find((task) => task.id === selectedTaskId) ?? null;
    }
    if (activeTab === "epics") {
      return epics.find((task) => task.id === selectedEpicId) ?? null;
    }
    const laneTasks = board[boardLane];
    return laneTasks.find((task) => task.id === selectedBoardTaskId) ?? laneTasks[0] ?? null;
  }, [
    activeTab,
    board,
    boardLane,
    epics,
    selectedBoardTaskId,
    selectedEpicId,
    selectedTaskId,
    tasks,
  ]);

  useKeyboard((key) => {
    if (key.name === "q" || key.name === "escape" || (key.ctrl && key.name === "c")) {
      renderer.destroy();
      return;
    }

    if (key.name === "r") {
      refreshNow();
      return;
    }

    if (key.name === "p") {
      setPaused((value) => !value);
      return;
    }

    if (key.name === "tab") {
      setActiveTab((value) => nextTab(TABS, value, 1));
      return;
    }

    if (key.name === "left") {
      setActiveTab((value) => nextTab(TABS, value, -1));
      return;
    }

    if (key.name === "right") {
      setActiveTab((value) => nextTab(TABS, value, 1));
      return;
    }

    if (activeTab === "board" && (key.name === "h" || key.name === "l")) {
      const direction: 1 | -1 = key.name === "l" ? 1 : -1;
      setBoardLane((lane) => nextTab(laneOrder(), lane, direction));
      return;
    }

    const direction =
      key.name === "down" || key.name === "j"
        ? 1
        : key.name === "up" || key.name === "k"
          ? -1
          : 0;

    if (direction === 0) {
      return;
    }

    if (activeTab === "tasks") {
      setSelectedTaskId((current) => nextItemId(tasks, current, direction));
      return;
    }

    if (activeTab === "epics") {
      setSelectedEpicId((current) => nextItemId(epics, current, direction));
      return;
    }

    const laneTasks = board[boardLane];
    setBoardSelection((current) => ({
      ...current,
      [boardLane]: nextItemId(laneTasks, current[boardLane], direction),
    }));
  });

  if (width < 72 || height < 20) {
    return (
      <box
        flexDirection="column"
        justifyContent="center"
        alignItems="center"
        flexGrow={1}
        backgroundColor={COLORS.shell}
      >
        <text fg={COLORS.warn}>Terminal too small.</text>
        <text fg={COLORS.muted}>Minimum size required: 72x20</text>
        <text fg={COLORS.muted}>{`Current size: ${width}x${height}`}</text>
      </box>
    );
  }

  const leftPaneWidth = Math.max(52, Math.floor(width * 0.68));
  const rightPaneWidth = Math.max(24, width - leftPaneWidth - 5);

  return (
    <box
      flexDirection="column"
      flexGrow={1}
      backgroundColor={COLORS.shell}
      paddingLeft={1}
      paddingRight={1}
      paddingTop={1}
      paddingBottom={1}
      gap={1}
    >
      <Header snapshot={snapshot} paused={paused} totalTasks={tasks.length} />
      <TabRow activeTab={activeTab} />
      <box flexDirection="row" flexGrow={1} gap={1}>
        <box width={leftPaneWidth} border borderColor={COLORS.muted} backgroundColor={COLORS.panel}>
          {activeTab === "tasks" ? (
            <TasksView tasks={tasks} selectedId={selectedTaskId} width={leftPaneWidth - 4} />
          ) : null}
          {activeTab === "epics" ? (
            <EpicsView
              tasks={tasks}
              epics={epics}
              selectedEpicId={selectedEpicId}
              width={leftPaneWidth - 4}
            />
          ) : null}
          {activeTab === "board" ? (
            <BoardView
              columns={board}
              lane={boardLane}
              selection={boardSelection}
              width={leftPaneWidth - 4}
            />
          ) : null}
        </box>

        <box width={rightPaneWidth} border borderColor={COLORS.muted} backgroundColor={COLORS.panel}>
          <Inspector task={selectedTask} width={rightPaneWidth - 4} />
        </box>
      </box>

      <Footer warnings={snapshot.warnings} />
    </box>
  );
}

function Header({
  snapshot,
  paused,
  totalTasks,
}: {
  snapshot: DataSnapshot;
  paused: boolean;
  totalTasks: number;
}) {
  return (
    <box flexDirection="column">
      <text fg={COLORS.text}>{`Tasque OpenTUI  source=${snapshot.source}  tasks=${totalTasks}`}</text>
      <text fg={COLORS.muted}>{`refreshed=${snapshot.loadedAt}  sync=${paused ? "paused" : "live"}`}</text>
    </box>
  );
}

function TabRow({ activeTab }: { activeTab: TabId }) {
  return (
    <box flexDirection="row" gap={1}>
      {TABS.map((tab) => {
        const isActive = tab === activeTab;
        return (
          <text key={tab} fg={isActive ? COLORS.active : COLORS.muted}>
            {isActive ? `[${labelForTab(tab)}]` : ` ${labelForTab(tab)} `}
          </text>
        );
      })}
    </box>
  );
}

function TasksView({
  tasks,
  selectedId,
  width,
}: {
  tasks: TaskRecord[];
  selectedId: string | null;
  width: number;
}) {
  const titleWidth = Math.max(14, width - 61);

  return (
    <box flexDirection="column" paddingLeft={1} paddingRight={1} paddingTop={1}>
      <text fg={COLORS.text}>Tasks</text>
      <text fg={COLORS.muted}>{renderTaskHeader(titleWidth)}</text>
      {tasks.slice(0, 200).map((task) => {
        const selected = task.id === selectedId;
        return (
          <text key={task.id} fg={selected ? COLORS.active : COLORS.text}>
            {renderTaskRow(task, selected, titleWidth)}
          </text>
        );
      })}
    </box>
  );
}

function EpicsView({
  tasks,
  epics,
  selectedEpicId,
  width,
}: {
  tasks: TaskRecord[];
  epics: TaskRecord[];
  selectedEpicId: string | null;
  width: number;
}) {
  const titleWidth = Math.max(14, width - 55);
  const selectedEpic = epics.find((item) => item.id === selectedEpicId) ?? epics[0] ?? null;

  return (
    <box flexDirection="column" paddingLeft={1} paddingRight={1} paddingTop={1}>
      <text fg={COLORS.text}>Epics</text>
      {selectedEpic ? (
        <text fg={COLORS.muted}>{renderEpicSummary(tasks, selectedEpic, width)}</text>
      ) : (
        <text fg={COLORS.muted}>No epic tasks found.</text>
      )}
      <text fg={COLORS.muted}>{renderEpicHeader(titleWidth)}</text>
      {epics.slice(0, 200).map((epic) => {
        const selected = epic.id === selectedEpicId;
        return (
          <text key={epic.id} fg={selected ? COLORS.active : COLORS.text}>
            {renderEpicRow(tasks, epic, selected, titleWidth)}
          </text>
        );
      })}
    </box>
  );
}

function BoardView({
  columns,
  lane,
  selection,
  width,
}: {
  columns: {
    open: TaskRecord[];
    in_progress: TaskRecord[];
    done: TaskRecord[];
  };
  lane: BoardLane;
  selection: BoardSelection;
  width: number;
}) {
  const colWidth = Math.max(16, Math.floor((width - 6) / 3));
  const rows = Math.max(columns.open.length, columns.in_progress.length, columns.done.length);

  return (
    <box flexDirection="column" paddingLeft={1} paddingRight={1} paddingTop={1}>
      <text fg={COLORS.text}>Board</text>
      <text fg={COLORS.muted}>{renderBoardHeader(lane, colWidth)}</text>
      {Array.from({ length: rows }).map((_, index) => {
        const open = columns.open[index] ?? null;
        const progress = columns.in_progress[index] ?? null;
        const done = columns.done[index] ?? null;
        const openSelected = lane === "open" && selection.open === open?.id;
        const progressSelected = lane === "in_progress" && selection.in_progress === progress?.id;
        const doneSelected = lane === "done" && selection.done === done?.id;

        return (
          <text key={`board-row-${index}`} fg={COLORS.text}>
            {`${renderBoardCell(open, openSelected, colWidth)} | ${renderBoardCell(progress, progressSelected, colWidth)} | ${renderBoardCell(done, doneSelected, colWidth)}`}
          </text>
        );
      })}
    </box>
  );
}

function Inspector({ task, width }: { task: TaskRecord | null; width: number }) {
  return (
    <box flexDirection="column" paddingLeft={1} paddingRight={1} paddingTop={1}>
      <text fg={COLORS.text}>Details</text>
      {!task ? <text fg={COLORS.muted}>No selection.</text> : null}
      {task ? <text fg={COLORS.text}>{`id=${task.id}`}</text> : null}
      {task ? <text fg={COLORS.text}>{`title=${truncate(task.title, Math.max(12, width - 6))}`}</text> : null}
      {task ? (
        <text fg={COLORS.text}>{`status=${task.status} kind=${task.kind} priority=P${task.priority}`}</text>
      ) : null}
      {task ? (
        <text fg={COLORS.text}>{`assignee=${task.assignee ?? "unassigned"} parent=${task.parent_id ?? "-"}`}</text>
      ) : null}
      {task ? (
        <text fg={COLORS.text}>{`planning=${task.planning_state ?? "needs_planning"}`}</text>
      ) : null}
      {task ? <text fg={specColor(task.spec_state)}>{`spec=${task.spec_state}`}</text> : null}
      {task ? (
        <text fg={COLORS.muted}>{`spec_detail=${truncate(task.spec_reason, Math.max(12, width - 12))}`}</text>
      ) : null}
      {task?.spec_path ? (
        <text fg={COLORS.muted}>{`spec_path=${truncate(task.spec_path, Math.max(12, width - 10))}`}</text>
      ) : null}
      {task ? <text fg={COLORS.muted}>{`updated=${task.updated_at}`}</text> : null}
      {task ? <text fg={COLORS.muted}>{`created=${task.created_at}`}</text> : null}
    </box>
  );
}

function Footer({ warnings }: { warnings: string[] }) {
  return (
    <box flexDirection="column">
      <text fg={COLORS.muted}>keys: q quit  tab/left/right tabs  up/down/j/k move  h/l board lane  r refresh  p pause</text>
      {warnings[0] ? <text fg={COLORS.warn}>{warnings[0]}</text> : null}
    </box>
  );
}

function renderTaskHeader(titleWidth: number): string {
  return `  ${pad("ID", 12)} ${pad("Type", 7)} ${pad("Title", titleWidth)} ${pad("Status", 12)} ${pad("P", 3)} ${pad("Spec", 9)}`;
}

function renderTaskRow(task: TaskRecord, selected: boolean, titleWidth: number): string {
  return `${selected ? ">" : " "} ${pad(task.id, 12)} ${pad(task.kind, 7)} ${pad(truncate(task.title, titleWidth), titleWidth)} ${pad(task.status, 12)} ${pad(`P${task.priority}`, 3)} ${pad(task.spec_state, 9)}`;
}

function renderEpicHeader(titleWidth: number): string {
  return `  ${pad("ID", 12)} ${pad("Title", titleWidth)} ${pad("Prog", 9)} ${pad("Status", 12)} ${pad("Spec", 9)}`;
}

function renderEpicRow(
  tasks: TaskRecord[],
  epic: TaskRecord,
  selected: boolean,
  titleWidth: number,
): string {
  const progress = epicProgress(tasks, epic.id);
  const meter = `${progress.done}/${progress.total}`;
  return `${selected ? ">" : " "} ${pad(epic.id, 12)} ${pad(truncate(epic.title, titleWidth), titleWidth)} ${pad(meter, 9)} ${pad(epic.status, 12)} ${pad(epic.spec_state, 9)}`;
}

function renderEpicSummary(tasks: TaskRecord[], epic: TaskRecord, width: number): string {
  const progress = epicProgress(tasks, epic.id);
  const label = `selected=${epic.id} ${truncate(epic.title, 26)}  progress=${progress.done}/${progress.total}  open=${progress.open}  in_progress=${progress.inProgress}`;
  return truncate(label, Math.max(20, width));
}

function renderBoardHeader(activeLane: BoardLane, colWidth: number): string {
  const open = activeLane === "open" ? `[Open]` : ` Open `;
  const progress = activeLane === "in_progress" ? `[In Progress]` : ` In Progress `;
  const done = activeLane === "done" ? `[Done]` : ` Done `;
  return `${pad(open, colWidth)} | ${pad(progress, colWidth)} | ${pad(done, colWidth)}`;
}

function renderBoardCell(task: TaskRecord | null, selected: boolean, width: number): string {
  if (!task) {
    return pad("", width);
  }
  const base = `${task.id} ${task.status} P${task.priority} ${task.spec_state}`;
  const body = truncate(base, Math.max(8, width - 2));
  return pad(`${selected ? ">" : " "}${body}`, width);
}

function labelForTab(tab: TabId): string {
  if (tab === "tasks") {
    return "Tasks";
  }
  if (tab === "epics") {
    return "Epics";
  }
  return "Board";
}

function specColor(specState: TaskRecord["spec_state"]): string {
  if (specState === "attached") {
    return COLORS.ok;
  }
  if (specState === "invalid") {
    return COLORS.danger;
  }
  return COLORS.warn;
}
