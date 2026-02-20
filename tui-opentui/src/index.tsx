import { createCliRenderer } from "@opentui/core";
import {
  createRoot,
  useKeyboard,
  useRenderer,
  useTerminalDimensions,
} from "@opentui/react";
import { useEffect, useMemo, useState } from "react";
import {
  type DependencyNode,
  fetchDependencyTree,
  fetchTasks,
  readConfigFromEnv,
} from "./data";
import {
  type BoardLane,
  type SpecState,
  type TabKey,
  type TasqueTask,
  KIND_COLORS,
  SPEC_COLORS,
  STATUS_COLORS,
  boardColumns,
  buildEpicProgress,
  computeSummary,
  sortTasks,
  specState,
  titleWithEllipsis,
} from "./model";

const THEME = {
  shellBg: "#0D1520",
  panelBg: "#111B29",
  raisedBg: "#162235",
  border: "#25364D",
  text: "#D7E3F4",
  muted: "#9DB0C8",
  dim: "#7388A3",
  focus: "#58A6FF",
  ok: "#7BC77E",
  row: "#101C2C",
  rowSelected: "#1A2C44",
  warning: "#F07178",
};

type SelectedByTab = Record<TabKey, number>;
type SelectedByLane = Record<BoardLane, number>;
const TAB_ORDER: TabKey[] = ["tasks", "epics", "board", "deps"];

interface TreeLine {
  task: TasqueTask;
  depth: number;
  isLastSibling: boolean;
  siblingTrail: boolean[];
}
interface TableLayout {
  idWidth: number;
  typeWidth: number;
  titleWidth: number;
  priorityWidth: number;
  specWidth: number;
  showSpec: boolean;
}
interface FilterPreset {
  id: string;
  label: string;
  statuses?: TasqueTask["status"][];
}
interface SpecDialogState {
  taskId: string;
  taskTitle: string;
  specPath: string;
  lines: string[];
  warning?: string;
  offset: number;
  loading: boolean;
}

function App() {
  const config = useMemo(() => readConfigFromEnv(), []);
  const renderer = useRenderer();
  const dimensions = useTerminalDimensions();

  const [tab, setTab] = useState<TabKey>(config.initialTab);
  const [snapshot, setSnapshot] = useState(() => fetchTasks(config));
  const [warning, setWarning] = useState<string | undefined>(snapshot.warning);
  const [selectedByTab, setSelectedByTab] = useState<SelectedByTab>({
    tasks: 0,
    epics: 0,
    board: 0,
    deps: 0,
  });
  const [lane, setLane] = useState<BoardLane>("open");
  const [selectedByLane, setSelectedByLane] = useState<SelectedByLane>({
    open: 0,
    in_progress: 0,
    done: 0,
  });
  const [specDialog, setSpecDialog] = useState<SpecDialogState | undefined>();
  const filterPresets = useMemo(() => buildFilterPresets(config.statusCsv), [config.statusCsv]);
  const [filterIndex, setFilterIndex] = useState(0);
  const [dependencyRoot, setDependencyRoot] = useState<DependencyNode | undefined>();
  const [dependencyWarning, setDependencyWarning] = useState<string | undefined>();

  useEffect(() => {
    setFilterIndex((current) => Math.min(current, filterPresets.length - 1));
  }, [filterPresets.length]);

  useEffect(() => {
    const refresh = () => {
      const next = fetchTasks(config);
      setSnapshot(next);
      setWarning(next.warning);
    };

    refresh();
    const timer = setInterval(refresh, config.intervalSeconds * 1000);
    return () => clearInterval(timer);
  }, [config]);

  const allTasks = useMemo(() => sortTasks(snapshot.tasks), [snapshot.tasks]);
  const activeFilter = filterPresets[filterIndex] ?? filterPresets[0];
  const filteredTasks = useMemo(
    () => applyTaskFilter(allTasks, activeFilter?.statuses),
    [activeFilter?.statuses, allTasks],
  );
  const summary = useMemo(() => computeSummary(filteredTasks), [filteredTasks]);
  const epicProgress = useMemo(() => buildEpicProgress(filteredTasks), [filteredTasks]);
  const board = useMemo(() => boardColumns(filteredTasks), [filteredTasks]);

  const visibleTasks = useMemo(() => {
    if (tab === "epics") {
      if (!epicProgress) {
        return [] as TasqueTask[];
      }
      return epicProgress.children.length > 0 ? epicProgress.children : [epicProgress.epic];
    }
    return filteredTasks;
  }, [tab, filteredTasks, epicProgress]);

  const treeLines = useMemo(() => buildTreeLines(filteredTasks), [filteredTasks]);
  const selectedIndex = selectedByTab[tab];
  const selectedTask = useMemo(() => {
    if (tab === "board") {
      const cards = board[lane];
      if (cards.length === 0) {
        return undefined;
      }
      const idx = Math.min(selectedByLane[lane], cards.length - 1);
      return cards[idx];
    }

    if (tab === "tasks") {
      if (treeLines.length === 0) {
        return undefined;
      }
      const idx = Math.min(selectedIndex, treeLines.length - 1);
      return treeLines[idx]?.task;
    }

    if (visibleTasks.length === 0) {
      return undefined;
    }
    const idx = Math.min(selectedIndex, visibleTasks.length - 1);
    return visibleTasks[idx];
  }, [board, lane, selectedByLane, selectedIndex, tab, treeLines, visibleTasks]);
  const rowCount = tab === "tasks" ? treeLines.length : visibleTasks.length;
  const contentWidth = Math.max(48, dimensions.width - 6);
  const tableRowBudget = Math.max(4, dimensions.height - 18);
  const specDialogBodyRows = Math.max(6, dimensions.height - 21);

  useEffect(() => {
    if (tab !== "deps") {
      return;
    }
    if (!selectedTask?.id) {
      setDependencyRoot(undefined);
      setDependencyWarning(undefined);
      return;
    }
    const dependency = fetchDependencyTree(config.tsqBin, selectedTask.id);
    setDependencyRoot(dependency.root);
    setDependencyWarning(dependency.warning);
  }, [config.tsqBin, selectedTask?.id, tab]);

  useEffect(() => {
    if (rowCount === 0 || tab === "board") {
      return;
    }
    setSelectedByTab((current) => ({
      ...current,
      [tab]: Math.min(current[tab], rowCount - 1),
    }));
  }, [rowCount, tab]);

  useEffect(() => {
    setSelectedByLane((current) => ({
      open: clampIndex(current.open, board.open.length),
      in_progress: clampIndex(current.in_progress, board.in_progress.length),
      done: clampIndex(current.done, board.done.length),
    }));
  }, [board.done.length, board.in_progress.length, board.open.length]);

  const openSpecDialogForTask = (task: TasqueTask) => {
    if (specState(task) !== "attached" || !task.spec_path) {
      return;
    }
    const specPath = task.spec_path;
    setSpecDialog({
      taskId: task.id,
      taskTitle: task.title,
      specPath,
      lines: ["Loading spec..."],
      offset: 0,
      loading: true,
    });
    void readSpecLines(specPath).then((result) => {
      setSpecDialog((current) => {
        if (!current || current.taskId !== task.id || current.specPath !== specPath) {
          return current;
        }
        return {
          ...current,
          lines: result.lines,
          warning: result.warning,
          offset: 0,
          loading: false,
        };
      });
    });
  };

  useKeyboard((key) => {
    if (key.ctrl && key.name === "c") {
      renderer.destroy();
      return;
    }

    if (specDialog) {
      const closeDialog =
        key.name === "escape" || key.name === "q" || key.name === "return" || key.name === "enter";
      if (closeDialog) {
        setSpecDialog(undefined);
        return;
      }

      const pageStep = Math.max(1, specDialogBodyRows - 1);
      let delta = 0;
      if (key.name === "up" || key.name === "k") {
        delta = -1;
      } else if (key.name === "down" || key.name === "j") {
        delta = 1;
      } else if (key.name === "pageup") {
        delta = -pageStep;
      } else if (key.name === "pagedown") {
        delta = pageStep;
      } else if (key.name === "home") {
        delta = -1_000_000;
      } else if (key.name === "end") {
        delta = 1_000_000;
      }

      if (delta !== 0) {
        setSpecDialog((current) => {
          if (!current) {
            return current;
          }
          const maxOffset = Math.max(0, current.lines.length - specDialogBodyRows);
          return {
            ...current,
            offset: clampNumber(current.offset + delta, 0, maxOffset),
          };
        });
      }
      return;
    }

    if (key.name === "escape" || key.name === "q") {
      renderer.destroy();
      return;
    }

    if (key.name === "r") {
      const next = fetchTasks(config);
      setSnapshot(next);
      setWarning(next.warning);
      return;
    }

    if (key.name === "f") {
      setFilterIndex((current) => (current + 1) % filterPresets.length);
      return;
    }

    if (key.name === "1") {
      setTab("tasks");
      return;
    }
    if (key.name === "2") {
      setTab("epics");
      return;
    }
    if (key.name === "3") {
      setTab("board");
      return;
    }
    if (key.name === "4") {
      setTab("deps");
      return;
    }

    if (key.name === "tab") {
      setTab((current) => nextTab(current, 1));
      return;
    }

    if (
      (key.name === "return" || key.name === "enter") &&
      selectedTask &&
      specState(selectedTask) === "attached" &&
      selectedTask.spec_path
    ) {
      openSpecDialogForTask(selectedTask);
      return;
    }

    if (tab === "board" && (key.name === "h" || key.name === "left")) {
      setLane((current) => previousLane(current));
      return;
    }

    if (tab === "board" && (key.name === "l" || key.name === "right")) {
      setLane((current) => nextLane(current));
      return;
    }

    const moveUp = key.name === "up" || key.name === "k";
    const moveDown = key.name === "down" || key.name === "j";
    if (!moveUp && !moveDown) {
      return;
    }

    const delta = moveUp ? -1 : 1;

    if (tab === "board") {
      setSelectedByLane((current) => {
        const cards = board[lane];
        if (cards.length === 0) {
          return current;
        }
        return {
          ...current,
          [lane]: clampIndex(current[lane] + delta, cards.length),
        };
      });
      return;
    }

    setSelectedByTab((current) => {
      if (rowCount === 0) {
        return current;
      }
      return {
        ...current,
        [tab]: clampIndex(current[tab] + delta, rowCount),
      };
    });
  });
  const itemBudget = tab === "tasks" ? Math.max(2, Math.floor(tableRowBudget / 2)) : tableRowBudget;
  const [start, end] = visibleRange(selectedIndex, rowCount, itemBudget);

  return (
    <box
      width="100%"
      height="100%"
      flexDirection="column"
      backgroundColor={THEME.shellBg}
      padding={1}
    >
      <box
        flexShrink={0}
        border
        borderColor={THEME.border}
        backgroundColor={THEME.panelBg}
        paddingX={1}
        paddingY={0}
      >
        <box flexDirection="column" width="100%">
          <box width="100%" justifyContent="space-between">
            <text>
              <span fg={THEME.text}>Tasque </span>
              <span fg={THEME.dim}>OpenTUI</span>
            </text>
            <text>
              <span fg={THEME.dim}>fetched </span>
              <span fg={THEME.muted}>{snapshot.fetchedAt}</span>
            </text>
          </box>
          <box width="100%" justifyContent="space-between">
            <text>
              <span fg={THEME.muted}>filter:</span>
              <span fg={THEME.text}> {activeFilter?.label ?? "active"}</span>
              {config.assignee ? (
                <>
                  <span fg={THEME.muted}> assignee:</span>
                  <span fg={THEME.text}> {config.assignee}</span>
                </>
              ) : null}
            </text>
            <text>
              <span fg={THEME.dim}>tasks {summary.total}</span>
              <span fg={THEME.dim}> open {summary.open}</span>
              <span fg={THEME.dim}> in_progress {summary.inProgress}</span>
              <span fg={THEME.dim}> blocked {summary.blocked}</span>
            </text>
          </box>
          {warning ? (
            <text>
              <span fg={THEME.warning}>warning: {warning}</span>
            </text>
          ) : null}
        </box>
      </box>

      <box
        marginTop={1}
        flexShrink={0}
        border
        borderColor={THEME.border}
        backgroundColor={THEME.panelBg}
      >
        <box flexDirection="row" paddingX={1} paddingY={0}>
          <TabChip tab={tab} value="tasks" label="Tasks" />
          <TabChip tab={tab} value="epics" label="Epics" />
          <TabChip tab={tab} value="board" label="Board" />
          <TabChip tab={tab} value="deps" label="Deps" />
        </box>
      </box>

      <box
        flexGrow={1}
        minHeight={0}
        marginTop={1}
      >
        <box
          flexGrow={1}
          minHeight={0}
          border
          borderColor={THEME.border}
          backgroundColor={THEME.panelBg}
          padding={1}
          flexDirection="column"
          overflow="hidden"
        >
          {specDialog ? (
            <SpecDialogView
              dialog={specDialog}
              width={contentWidth}
              bodyRows={specDialogBodyRows}
            />
          ) : (
            <>
              {tab === "tasks" ? (
                <TreeView
                  lines={treeLines.slice(start, end)}
                  selectedTaskId={selectedTask?.id}
                  width={contentWidth}
                />
              ) : null}

              {tab === "epics" ? (
                <EpicsView
                  tasks={visibleTasks.slice(start, end)}
                  selectedTaskId={selectedTask?.id}
                  epicProgress={epicProgress}
                  width={contentWidth}
                />
              ) : null}

              {tab === "board" ? (
                <BoardView
                  tasks={board}
                  lane={lane}
                  selectedByLane={selectedByLane}
                  rowBudget={tableRowBudget}
                />
              ) : null}

              {tab === "deps" ? (
                <DepsView
                  root={dependencyRoot}
                  warning={dependencyWarning}
                  width={contentWidth}
                  selectedTaskId={selectedTask?.id}
                  rowBudget={tableRowBudget}
                />
              ) : null}
            </>
          )}
        </box>
      </box>

      <box
        marginTop={1}
        flexShrink={0}
        border
        borderColor={THEME.border}
        backgroundColor={THEME.panelBg}
      >
        <text>
          <span fg={THEME.dim}>
            {specDialog
              ? "esc/enter/q close spec  j/k or up/down scroll  pgup/pgdn page  home/end jump"
              : "q/esc quit  tab 1/2/3/4 switch view  j/k or up/down move  h/l board lane  f filter  enter open spec  r refresh"}
          </span>
        </text>
      </box>
    </box>
  );
}

function TabChip({
  tab,
  value,
  label,
}: {
  tab: TabKey;
  value: TabKey;
  label: string;
}) {
  const active = tab === value;
  return (
    <text>
      <span fg={active ? THEME.focus : THEME.muted}>{active ? `[${label}]` : ` ${label} `}</span>
      <span fg={THEME.dim}> </span>
    </text>
  );
}

function TasksTable({
  tasks,
  selectedTaskId,
  width,
}: {
  tasks: TasqueTask[];
  selectedTaskId?: string;
  width: number;
}) {
  const layout = useMemo(() => buildTableLayout(width), [width]);
  return (
    <box flexDirection="column" gap={0}>
      <text>
        <span fg={THEME.dim}>{tableHeader(layout)}</span>
      </text>
      {tasks.map((task) => (
        <TaskRow
          key={task.id}
          task={task}
          selected={task.id === selectedTaskId}
          layout={layout}
        />
      ))}
      {tasks.length === 0 ? (
        <text>
          <span fg={THEME.dim}>No tasks in current filter</span>
        </text>
      ) : null}
    </box>
  );
}

function EpicsView({
  tasks,
  selectedTaskId,
  epicProgress,
  width,
}: {
  tasks: TasqueTask[];
  selectedTaskId?: string;
  epicProgress: ReturnType<typeof buildEpicProgress>;
  width: number;
}) {
  const layout = useMemo(() => buildTableLayout(width), [width]);
  return (
    <box flexDirection="column" gap={0}>
      {epicProgress ? (
        <box marginBottom={1} border borderColor={THEME.border} backgroundColor={THEME.raisedBg}>
          <text>
            <span fg={THEME.text}>{epicProgress.epic.id}</span>
            <span fg={THEME.muted}> {titleWithEllipsis(epicProgress.epic.title, 48)}</span>
            <span fg={THEME.dim}> {renderMeter(epicProgress.done, epicProgress.children.length, 12)}</span>
            <span fg={THEME.muted}> {epicProgress.done}/{epicProgress.children.length}</span>
          </text>
        </box>
      ) : null}
      <text>
        <span fg={THEME.dim}>{tableHeader(layout)}</span>
      </text>
      {tasks.map((task) => (
        <TaskRow
          key={task.id}
          task={task}
          selected={task.id === selectedTaskId}
          layout={layout}
        />
      ))}
      {tasks.length === 0 ? (
        <text>
          <span fg={THEME.dim}>No epic tasks found</span>
        </text>
      ) : null}
    </box>
  );
}

function TaskRow({
  task,
  selected,
  layout,
}: {
  task: TasqueTask;
  selected: boolean;
  layout: TableLayout;
}) {
  const spec = specState(task);
  return (
    <box backgroundColor={selected ? THEME.rowSelected : THEME.row}>
      <text>
        <span fg={STATUS_COLORS[task.status]}>{statusIcon(task.status)} </span>
        <span fg={THEME.text}>{pad(task.id, layout.idWidth)} </span>
        <span fg={KIND_COLORS[task.kind]}>{pad(kindLabel(task.kind), layout.typeWidth)} </span>
        <span fg={THEME.text}>
          {pad(titleWithEllipsis(task.title, layout.titleWidth), layout.titleWidth)}{" "}
        </span>
        <span fg={THEME.dim}> </span>
        <span fg={THEME.dim}>{pad(`P${task.priority}`, layout.priorityWidth)}</span>
        {layout.showSpec ? (
          <>
            <span fg={THEME.dim}> </span>
            <SpecPill state={spec} />
          </>
        ) : null}
      </text>
    </box>
  );
}

function BoardView({
  tasks,
  lane,
  selectedByLane,
  rowBudget,
}: {
  tasks: Record<BoardLane, TasqueTask[]>;
  lane: BoardLane;
  selectedByLane: SelectedByLane;
  rowBudget: number;
}) {
  return (
    <box flexDirection="row" gap={1} flexGrow={1}>
      <BoardColumn
        title="Open"
        cards={tasks.open}
        active={lane === "open"}
        selectedIndex={selectedByLane.open}
        rowBudget={rowBudget}
      />
      <BoardColumn
        title="In Progress"
        cards={tasks.in_progress}
        active={lane === "in_progress"}
        selectedIndex={selectedByLane.in_progress}
        rowBudget={rowBudget}
      />
      <BoardColumn
        title="Done"
        cards={tasks.done}
        active={lane === "done"}
        selectedIndex={selectedByLane.done}
        rowBudget={rowBudget}
      />
    </box>
  );
}

function BoardColumn({
  title,
  cards,
  active,
  selectedIndex,
  rowBudget,
}: {
  title: string;
  cards: TasqueTask[];
  active: boolean;
  selectedIndex: number;
  rowBudget: number;
}) {
  const visibleCards = cards.slice(0, rowBudget);
  const overflow = cards.length - visibleCards.length;
  return (
    <box
      flexGrow={1}
      border
      borderColor={active ? THEME.focus : THEME.border}
      backgroundColor={THEME.raisedBg}
      flexDirection="column"
      paddingX={1}
    >
      <text>
        <span fg={active ? THEME.focus : THEME.text}>{title}</span>
        <span fg={THEME.dim}> ({cards.length})</span>
      </text>
      {visibleCards.map((task, idx) => {
        const selected = active && idx === selectedIndex;
        const spec = specState(task);
        return (
          <box key={task.id} backgroundColor={selected ? THEME.rowSelected : THEME.row}>
            <text>
              <span fg={THEME.text}>{titleWithEllipsis(task.title, 24)}</span>
              <br />
              <span fg={KIND_COLORS[task.kind]}>{kindLabel(task.kind)} </span>
              <span fg={THEME.dim}>[P{task.priority}] </span>
              <SpecPill state={spec} compact />
              <br />
              <span fg={STATUS_COLORS[task.status]}>{statusIcon(task.status)} </span>
              <span fg={THEME.dim}>{task.id}</span>
            </text>
          </box>
        );
      })}
      {cards.length === 0 ? (
        <text>
          <span fg={THEME.dim}>No tasks</span>
        </text>
      ) : null}
      {overflow > 0 ? (
        <text>
          <span fg={THEME.dim}>+{overflow} more</span>
        </text>
      ) : null}
    </box>
  );
}

function TreeView({
  lines,
  selectedTaskId,
  width,
}: {
  lines: TreeLine[];
  selectedTaskId?: string;
  width: number;
}) {
  const titleWidth = Math.max(18, width - 18);
  return (
    <box flexDirection="column" gap={0}>
      {lines.map((line) => {
        const selected = line.task.id === selectedTaskId;
        const spec = specState(line.task);
        const displayId = treeDisplayId(line.task.id, line.depth);
        const titlePrefix = buildTreePrefix(line);
        const metadataLead = " ".repeat(2 + 13 + 1 + titlePrefix.length);
        return (
          <box
            key={`${line.task.id}-tree`}
            backgroundColor={selected ? THEME.rowSelected : THEME.row}
          >
            <text>
              <span fg={STATUS_COLORS[line.task.status]}>{statusIcon(line.task.status)} </span>
              <span fg={THEME.text}>{pad(displayId, 13)} </span>
              <span fg={THEME.dim}>{titlePrefix}</span>
              <strong>
                <span fg={THEME.text}>
                  {titleWithEllipsis(line.task.title, titleWidth)}
                </span>
              </strong>
              <br />
              <span fg={THEME.dim}>{metadataLead}</span>
              <span fg={THEME.muted}>P{line.task.priority}</span>
              <span fg={THEME.dim}> | </span>
              <span fg={line.task.planning_state === "planned" ? THEME.ok : THEME.warning}>
                {planningLabel(line.task.planning_state)}
              </span>
              <span fg={THEME.dim}> | </span>
              <span fg={SPEC_COLORS[spec]}>{specLabel(spec)}</span>
              <span fg={THEME.dim}> | </span>
              <span fg={THEME.muted}>Updated : </span>
              <span fg={THEME.dim}>{formatUpdatedAt(line.task.updated_at)}</span>
            </text>
          </box>
        );
      })}
      {lines.length === 0 ? (
        <text>
          <span fg={THEME.dim}>No tasks in current filter</span>
        </text>
      ) : null}
    </box>
  );
}

function DepsView({
  root,
  warning,
  width,
  selectedTaskId,
  rowBudget,
}: {
  root?: DependencyNode;
  warning?: string;
  width: number;
  selectedTaskId?: string;
  rowBudget: number;
}) {
  const lines = useMemo(() => flattenDependencyTree(root), [root]);
  const visibleLines = lines.slice(0, rowBudget);
  const overflow = lines.length - visibleLines.length;
  const lineWidth = Math.max(24, width - 2);
  return (
    <box flexDirection="column" gap={0}>
      <text>
        <span fg={THEME.text}>Dependencies</span>
        <span fg={THEME.dim}> selected: {selectedTaskId ?? "-"}</span>
      </text>
      {warning ? (
        <text>
          <span fg={THEME.warning}>{warning}</span>
        </text>
      ) : null}
      {visibleLines.map((line) => (
        <text key={line.key}>
          <span fg={THEME.muted}>{titleWithEllipsis(line.text, lineWidth)}</span>
        </text>
      ))}
      {overflow > 0 ? (
        <text>
          <span fg={THEME.dim}>+{overflow} more</span>
        </text>
      ) : null}
      {!warning && lines.length === 0 ? (
        <text>
          <span fg={THEME.dim}>No dependency edges found</span>
        </text>
      ) : null}
    </box>
  );
}

function SpecDialogView({
  dialog,
  width,
  bodyRows,
}: {
  dialog: SpecDialogState;
  width: number;
  bodyRows: number;
}) {
  const total = dialog.lines.length;
  const start = clampNumber(dialog.offset, 0, Math.max(0, total - bodyRows));
  const end = Math.min(total, start + bodyRows);
  const visibleLines = dialog.loading ? ["Loading spec..."] : dialog.lines.slice(start, end);
  const titleWidth = Math.max(24, width - 6);
  return (
    <box
      border
      borderColor={THEME.focus}
      backgroundColor={THEME.raisedBg}
      flexDirection="column"
      paddingX={1}
      paddingY={0}
      marginTop={1}
    >
      <text>
        <span fg={THEME.focus}>Spec</span>
        <span fg={THEME.text}> {dialog.taskId}</span>
        <span fg={THEME.dim}> {titleWithEllipsis(dialog.taskTitle, 36)}</span>
      </text>
      <text>
        <span fg={THEME.muted}>{titleWithEllipsis(dialog.specPath, titleWidth)}</span>
      </text>
      {dialog.warning ? (
        <text>
          <span fg={THEME.warning}>{dialog.warning}</span>
        </text>
      ) : (
        visibleLines.map((line, idx) => (
          <text key={`${dialog.specPath}:${start + idx}`}>
            <span fg={THEME.text}>{titleWithEllipsis(line, titleWidth)}</span>
          </text>
        ))
      )}
      <text>
        <span fg={THEME.dim}>
          {dialog.warning
            ? "esc/enter/q close"
            : `lines ${total === 0 ? 0 : start + 1}-${end} of ${total}`}
        </span>
      </text>
    </box>
  );
}

function SpecPill({
  state,
  compact,
}: {
  state: SpecState;
  compact?: boolean;
}) {
  const label = compact ? state : `[${state}]`;
  return <span fg={SPEC_COLORS[state]}>{label}</span>;
}

function statusIcon(status: TasqueTask["status"]): string {
  switch (status) {
    case "open":
    case "deferred":
      return "○";
    case "in_progress":
    case "blocked":
      return "◐";
    case "closed":
    case "canceled":
      return "●";
  }
}

function kindLabel(kind: TasqueTask["kind"]): string {
  switch (kind) {
    case "task":
      return "Task";
    case "feature":
      return "Feature";
    case "epic":
      return "Epic";
  }
}

function planningLabel(value: TasqueTask["planning_state"] | undefined): string {
  return value === "planned" ? "Planned" : "Needs planning";
}

function specLabel(value: SpecState): string {
  switch (value) {
    case "attached":
      return "Spec Attached";
    case "missing":
      return "No Spec";
    case "invalid":
      return "Spec Invalid";
  }
}

function formatUpdatedAt(iso: string): string {
  const parsed = new Date(iso);
  if (Number.isNaN(parsed.getTime())) {
    return iso;
  }
  const shortMonths = [
    "Jan",
    "Feb",
    "Mar",
    "Apr",
    "May",
    "Jun",
    "Jul",
    "Aug",
    "Sep",
    "Oct",
    "Nov",
    "Dec",
  ];
  const year = `${parsed.getFullYear()}`.slice(2);
  const month = shortMonths[parsed.getMonth()] ?? "???";
  const day = `${parsed.getDate()}`.padStart(2, "0");
  return `${day} ${month} ${year}`;
}

function treeDisplayId(taskId: string, depth: number): string {
  if (depth <= 0) {
    return taskId;
  }
  const dotIndex = taskId.indexOf(".");
  if (dotIndex >= 0 && dotIndex < taskId.length - 1) {
    return taskId.slice(dotIndex);
  }
  return taskId;
}

function buildTreePrefix(line: TreeLine): string {
  if (line.depth <= 0) {
    return "";
  }
  const ancestors = buildAncestorPrefix(line.siblingTrail);
  const own = line.isLastSibling ? "└─" : "├─";
  return `${ancestors}${own} `;
}

function buildAncestorPrefix(siblingTrail: boolean[]): string {
  return siblingTrail.map((hasMoreSiblings) => (hasMoreSiblings ? "│ " : "  ")).join("");
}

async function readSpecLines(
  specPath: string,
): Promise<{ lines: string[]; warning?: string }> {
  try {
    const text = await Bun.file(specPath).text();
    if (text.length === 0) {
      return { lines: ["(empty spec)"] };
    }
    return {
      lines: text.replaceAll("\r\n", "\n").split("\n"),
    };
  } catch (error) {
    return {
      lines: [],
      warning: `Failed to open spec: ${errorMessage(error)}`,
    };
  }
}

function errorMessage(error: unknown): string {
  if (error instanceof Error && error.message) {
    return error.message;
  }
  return "unknown error";
}

function buildFilterPresets(statusCsv: string): FilterPreset[] {
  const customStatuses = parseStatusCsv(statusCsv);
  const presets: FilterPreset[] = [];
  if (customStatuses.length > 0) {
    presets.push({
      id: "custom",
      label: `custom:${customStatuses.join(",")}`,
      statuses: customStatuses,
    });
  } else {
    presets.push({
      id: "active",
      label: "active",
      statuses: ["open", "in_progress"],
    });
  }
  presets.push({ id: "open", label: "open", statuses: ["open"] });
  presets.push({ id: "in_progress", label: "in_progress", statuses: ["in_progress"] });
  presets.push({ id: "closed", label: "closed", statuses: ["closed"] });
  presets.push({ id: "canceled", label: "canceled", statuses: ["canceled"] });
  presets.push({ id: "done", label: "done", statuses: ["closed", "canceled"] });
  presets.push({ id: "full", label: "full", statuses: undefined });
  return presets;
}

function parseStatusCsv(value: string): TasqueTask["status"][] {
  const allowed = new Set<TasqueTask["status"]>([
    "open",
    "in_progress",
    "blocked",
    "deferred",
    "closed",
    "canceled",
  ]);
  const parsed: TasqueTask["status"][] = [];
  for (const token of value.split(",")) {
    const status = token.trim() as TasqueTask["status"];
    if (!status || !allowed.has(status) || parsed.includes(status)) {
      continue;
    }
    parsed.push(status);
  }
  return parsed;
}

function applyTaskFilter(
  tasks: TasqueTask[],
  statuses: TasqueTask["status"][] | undefined,
): TasqueTask[] {
  if (!statuses || statuses.length === 0) {
    return tasks;
  }
  return tasks.filter((task) => statuses.includes(task.status));
}

function buildTreeLines(tasks: TasqueTask[]): TreeLine[] {
  const byParent = new Map<string, TasqueTask[]>();
  const byId = new Map(tasks.map((task) => [task.id, task]));

  for (const task of tasks) {
    const parent = task.parent_id;
    if (!parent || !byId.has(parent)) {
      continue;
    }
    const list = byParent.get(parent) ?? [];
    list.push(task);
    byParent.set(parent, list);
  }

  const roots = tasks.filter((task) => !task.parent_id || !byId.has(task.parent_id));
  const output: TreeLine[] = [];

  const walk = (
    task: TasqueTask,
    depth: number,
    siblingTrail: boolean[],
    isLastSibling: boolean,
  ) => {
    const children = byParent.get(task.id) ?? [];
    output.push({
      task,
      depth,
      isLastSibling,
      siblingTrail,
    });
    children.forEach((child, index) => {
      walk(child, depth + 1, [...siblingTrail, index < children.length - 1], index === children.length - 1);
    });
  };

  for (const root of roots) {
    const rootChildren = roots;
    const rootIndex = rootChildren.indexOf(root);
    walk(root, 0, [], rootIndex >= rootChildren.length - 1);
  }

  return output;
}

function flattenDependencyTree(root: DependencyNode | undefined): Array<{ key: string; text: string }> {
  if (!root) {
    return [];
  }
  const lines: Array<{ key: string; text: string }> = [];
  const walk = (node: DependencyNode, depth: number, path: string) => {
    const title = node.task?.title ? ` ${node.task.title}` : "";
    const status = node.task?.status ? `${statusIcon(node.task.status)} ` : "";
    const edge =
      node.depType && node.direction ? ` (${node.depType}, ${node.direction})` : "";
    const indent = " ".repeat(depth * 2);
    lines.push({
      key: `${path}:${node.id}:${depth}`,
      text: `${indent}${status}${node.id}${edge}${title}`,
    });
    node.children.forEach((child, index) => walk(child, depth + 1, `${path}.${index}`));
  };
  walk(root, 0, "root");
  return lines;
}

function tableHeader(layout: TableLayout): string {
  const parts = [
    pad("ID", layout.idWidth),
    pad("Type", layout.typeWidth),
    pad("Title", layout.titleWidth),
  ];
  parts.push(pad("Pr", layout.priorityWidth));
  if (layout.showSpec) {
    parts.push("Spec");
  }
  return parts.join(" ");
}

function buildTableLayout(width: number): TableLayout {
  const layout: TableLayout = {
    idWidth: 12,
    typeWidth: 8,
    titleWidth: 24,
    priorityWidth: 3,
    specWidth: 9,
    showSpec: true,
  };

  const minTitle = 14;
  const calcTitleWidth = () => {
    const widths = [2, layout.idWidth, layout.typeWidth, layout.priorityWidth];
    if (layout.showSpec) {
      widths.push(layout.specWidth);
    }
    const separators = widths.length;
    const used = widths.reduce((sum, value) => sum + value, 0) + separators;
    return width - used;
  };

  while (true) {
    const titleWidth = calcTitleWidth();
    if (titleWidth >= minTitle) {
      layout.titleWidth = titleWidth;
      break;
    }
    if (layout.showSpec) {
      layout.showSpec = false;
      continue;
    }
    layout.titleWidth = Math.max(10, titleWidth);
    break;
  }

  return layout;
}

function pad(value: string, width: number): string {
  if (value.length >= width) {
    return value.slice(0, width);
  }
  return `${value}${" ".repeat(width - value.length)}`;
}

function renderMeter(done: number, total: number, width: number): string {
  if (total <= 0) {
    return `[${"░".repeat(width)}]`;
  }
  const filled = Math.round((done / total) * width);
  const clamped = Math.min(width, Math.max(0, filled));
  return `[${"█".repeat(clamped)}${"░".repeat(width - clamped)}]`;
}

function clampIndex(next: number, size: number): number {
  if (size <= 0) {
    return 0;
  }
  if (next < 0) {
    return 0;
  }
  if (next >= size) {
    return size - 1;
  }
  return next;
}

function clampNumber(value: number, min: number, max: number): number {
  if (value < min) {
    return min;
  }
  if (value > max) {
    return max;
  }
  return value;
}

function visibleRange(selectedIndex: number, total: number, budget: number): [number, number] {
  if (total <= budget) {
    return [0, total];
  }
  const half = Math.floor(budget / 2);
  let start = Math.max(0, selectedIndex - half);
  let end = start + budget;
  if (end > total) {
    end = total;
    start = end - budget;
  }
  return [start, end];
}

function nextTab(tab: TabKey, direction: 1 | -1): TabKey {
  const index = TAB_ORDER.indexOf(tab);
  const safe = index >= 0 ? index : 0;
  const next = (safe + direction + TAB_ORDER.length) % TAB_ORDER.length;
  return TAB_ORDER[next]!;
}

function previousLane(lane: BoardLane): BoardLane {
  if (lane === "done") {
    return "in_progress";
  }
  if (lane === "in_progress") {
    return "open";
  }
  return "done";
}

function nextLane(lane: BoardLane): BoardLane {
  if (lane === "open") {
    return "in_progress";
  }
  if (lane === "in_progress") {
    return "done";
  }
  return "open";
}

const renderer = await createCliRenderer();
createRoot(renderer).render(<App />);
