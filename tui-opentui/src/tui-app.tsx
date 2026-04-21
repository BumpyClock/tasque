import { useKeyboard, useRenderer, useTerminalDimensions } from "@opentui/react";
import { useEffect, useMemo, useState } from "react";
import {
  type DependencyNode,
  fetchDependencyTree,
  fetchTasks,
  readConfigFromEnv,
} from "./data";
import {
  type BoardLane,
  type TabKey,
  type TasqueTask,
  boardColumns,
  buildEpicProgress,
  computeSummary,
  sortTasks,
  specState,
} from "./model";
import {
  THEME,
  applyTaskFilter,
  buildFilterPresets,
  buildTreeLines,
  clampIndex,
  clampNumber,
  nextLane,
  nextTab,
  previousLane,
  readSpecLines,
  visibleRange,
} from "./tui-helpers";
import type { SelectedByLane, SelectedByTab, SpecDialogState } from "./tui-types";
import { BoardView, DepsView, EpicsView, SpecDialogView, TabChip, TreeView } from "./tui-views";

export function App() {
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

      <box marginTop={1} flexShrink={0} border borderColor={THEME.border} backgroundColor={THEME.panelBg}>
        <box flexDirection="row" paddingX={1} paddingY={0}>
          <TabChip tab={tab} value="tasks" label="Tasks" />
          <TabChip tab={tab} value="epics" label="Epics" />
          <TabChip tab={tab} value="board" label="Board" />
          <TabChip tab={tab} value="deps" label="Deps" />
        </box>
      </box>

      <box flexGrow={1} minHeight={0} marginTop={1}>
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
            <SpecDialogView dialog={specDialog} width={contentWidth} bodyRows={specDialogBodyRows} />
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

      <box marginTop={1} flexShrink={0} border borderColor={THEME.border} backgroundColor={THEME.panelBg}>
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
