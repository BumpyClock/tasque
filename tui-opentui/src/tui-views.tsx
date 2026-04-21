import { useMemo } from "react";
import type { DependencyNode } from "./data";
import {
  type BoardLane,
  type EpicProgress,
  type SpecState,
  type TabKey,
  type TasqueTask,
  KIND_COLORS,
  SPEC_COLORS,
  STATUS_COLORS,
  specState,
  titleWithEllipsis,
} from "./model";
import {
  THEME,
  buildTableLayout,
  buildTreePrefix,
  flattenDependencyTree,
  formatUpdatedAt,
  kindLabel,
  pad,
  planningLabel,
  renderMeter,
  specLabel,
  statusIcon,
  tableHeader,
  treeDisplayId,
} from "./tui-helpers";
import type { SelectedByLane, SpecDialogState, TableLayout, TreeLine } from "./tui-types";

export function TabChip({
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

export function EpicsView({
  tasks,
  selectedTaskId,
  epicProgress,
  width,
}: {
  tasks: TasqueTask[];
  selectedTaskId?: string;
  epicProgress: EpicProgress | null;
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
        <TaskRow key={task.id} task={task} selected={task.id === selectedTaskId} layout={layout} />
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

export function BoardView({
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

export function TreeView({
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
                <span fg={THEME.text}>{titleWithEllipsis(line.task.title, titleWidth)}</span>
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

export function DepsView({
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

export function SpecDialogView({
  dialog,
  width,
  bodyRows,
}: {
  dialog: SpecDialogState;
  width: number;
  bodyRows: number;
}) {
  const total = dialog.lines.length;
  const start = Math.max(0, Math.min(dialog.offset, Math.max(0, total - bodyRows)));
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
