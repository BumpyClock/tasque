import { useKeyboard, useRenderer, useTerminalDimensions } from "@opentui/react";
import { useEffect, useMemo, useRef, useState } from "react";
import {
	type DataSnapshot,
	createInFlightGuard,
	fetchTasks,
	readConfigFromEnv,
} from "./data";
import {
	STATUS_COLORS,
	computeSummary,
	titleWithEllipsis,
} from "./model";
import {
	collapsibleTaskIds,
	idColumnWidth,
	resolveSelectionIndex,
	treeMarker,
} from "./tree";
import { THEME, clampIndex, pad, statusIcon, visibleRange } from "./tui-helpers";
import {
	buildWatchRows,
	filterLabel,
	metaBadge,
	readWatchTreeFlag,
	watchListRowBudget,
} from "./watch-helpers";

export function WatchApp() {
	const config = useMemo(() => readConfigFromEnv(), []);
	const tree = useMemo(() => readWatchTreeFlag(), []);
	const renderer = useRenderer();
	const dimensions = useTerminalDimensions();

	const [snapshot, setSnapshot] = useState<DataSnapshot>({
		fetchedAt: "-",
		tasks: [],
	});
	const [warning, setWarning] = useState<string | undefined>();
	const [paused, setPaused] = useState(false);
	const [selectedIndex, setSelectedIndex] = useState(0);
	const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());

	const pausedRef = useRef(paused);
	pausedRef.current = paused;
	const refreshRef = useRef<() => void>(() => {});

	useEffect(() => {
		let cancelled = false;
		const guardedFetch = createInFlightGuard(() => fetchTasks(config));

		const refresh = async () => {
			const next = await guardedFetch();
			if (cancelled || !next) {
				// Skipped poll (a previous fetch is still in flight) or torn down.
				return;
			}
			setSnapshot(next);
			setWarning(next.warning);
		};

		// Manual refresh (`r` / unpause) always fetches, ignoring the pause gate.
		refreshRef.current = () => {
			void refresh();
		};

		void refresh();
		const timer = setInterval(() => {
			if (pausedRef.current) {
				return;
			}
			void refresh();
		}, config.intervalSeconds * 1000);
		return () => {
			cancelled = true;
			clearInterval(timer);
		};
	}, [config]);

	const rows = useMemo(
		() => buildWatchRows(snapshot.tasks, tree, collapsed),
		[collapsed, snapshot.tasks, tree],
	);
	// Sized from every fetched task (not just visible rows) so the column does
	// not jitter when folds or scrolling change which IDs are on screen.
	const idWidth = useMemo(
		() => idColumnWidth(snapshot.tasks.map((task) => task.id)),
		[snapshot.tasks],
	);
	const summary = useMemo(
		() => computeSummary(snapshot.tasks),
		[snapshot.tasks],
	);
	const filter = useMemo(
		() => filterLabel(config.statusCsv, config.assignee),
		[config.assignee, config.statusCsv],
	);

	// Keep the selection valid as the live list grows or shrinks between polls.
	useEffect(() => {
		setSelectedIndex((current) => clampIndex(current, rows.length));
	}, [rows.length]);

	const contentWidth = Math.max(40, dimensions.width - 6);
	const rowBudget = watchListRowBudget(dimensions.height, Boolean(warning));
	const [start, end] = visibleRange(selectedIndex, rows.length, rowBudget);
	const visibleRows = rows.slice(start, end);

	// Apply a new fold set and remap selection so the same task (or its nearest
	// visible ancestor, when it just got hidden) stays selected.
	const applyCollapsed = (next: ReadonlySet<string>, followId?: string) => {
		const nextRows = buildWatchRows(snapshot.tasks, tree, next);
		const targetId = followId ?? rows[selectedIndex]?.task.id;
		setCollapsed(next);
		setSelectedIndex(
			resolveSelectionIndex(
				nextRows.map((row) => row.task.id),
				targetId,
				snapshot.tasks,
				selectedIndex,
			),
		);
	};

	const toggleFold = (row: (typeof rows)[number] | undefined) => {
		if (!row?.hasChildren) {
			return;
		}
		const next = new Set(collapsed);
		if (next.has(row.task.id)) {
			next.delete(row.task.id);
		} else {
			next.add(row.task.id);
		}
		applyCollapsed(next);
	};

	useKeyboard((key) => {
		if (key.ctrl && key.name === "c") {
			renderer.destroy();
			return;
		}
		if (key.name === "escape" || key.name === "q") {
			renderer.destroy();
			return;
		}
		if (key.name === "r") {
			refreshRef.current();
			return;
		}

		if (tree) {
			const selectedRow = rows[selectedIndex];
			if (key.name === "space") {
				toggleFold(selectedRow);
				return;
			}
			if (key.name === "left" || key.name === "h") {
				if (selectedRow?.hasChildren && !selectedRow.isCollapsed) {
					toggleFold(selectedRow);
				} else if (selectedRow?.task.parent_id) {
					// Leaf or already folded: jump to the parent row.
					applyCollapsed(collapsed, selectedRow.task.parent_id);
				}
				return;
			}
			if (key.name === "right" || key.name === "l") {
				if (selectedRow?.isCollapsed) {
					toggleFold(selectedRow);
				} else if (selectedRow?.hasChildren) {
					// Already expanded: step into the first child.
					setSelectedIndex((current) =>
						clampIndex(current + 1, rows.length),
					);
				}
				return;
			}
			if (key.name === "-") {
				applyCollapsed(collapsibleTaskIds(snapshot.tasks));
				return;
			}
			if (key.name === "=" || key.name === "+") {
				applyCollapsed(new Set());
				return;
			}
		}
		if (key.name === "p") {
			setPaused((current) => {
				const next = !current;
				if (!next) {
					// Resuming: refresh immediately so the view is current.
					refreshRef.current();
				}
				return next;
			});
			return;
		}
		if (key.name === "g") {
			// `G` (shift+g) jumps to the bottom, `g` to the top.
			setSelectedIndex(key.shift ? Math.max(0, rows.length - 1) : 0);
			return;
		}
		if (key.name === "home") {
			setSelectedIndex(0);
			return;
		}
		if (key.name === "end") {
			setSelectedIndex(Math.max(0, rows.length - 1));
			return;
		}

		const moveUp = key.name === "up" || key.name === "k";
		const moveDown = key.name === "down" || key.name === "j";
		if (!moveUp && !moveDown) {
			return;
		}
		const delta = moveUp ? -1 : 1;
		setSelectedIndex((current) => clampIndex(current + delta, rows.length));
	});

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
				flexDirection="column"
			>
				<box width="100%" justifyContent="space-between">
					<text>
						<span fg={THEME.focus}>[tsq watch]</span>
						<span fg={THEME.dim}> interval={config.intervalSeconds}s</span>
						{paused ? <span fg={THEME.warning}> ⏸ paused</span> : null}
					</text>
					<text>
						<span fg={THEME.dim}>refreshed </span>
						<span fg={THEME.muted}>{snapshot.fetchedAt}</span>
					</text>
				</box>
				<box width="100%">
					<text>
						<span fg={THEME.muted}>filter=</span>
						<span fg={THEME.text}>{filter}</span>
					</text>
				</box>
				<box width="100%">
					<text>
						<span fg={THEME.text}>active {summary.total}</span>
						<span fg={THEME.dim}>  in_progress {summary.inProgress}</span>
						<span fg={THEME.dim}>  open {summary.open}</span>
						<span fg={THEME.dim}>  blocked {summary.blocked}</span>
					</text>
				</box>
				{warning ? (
					<box width="100%">
						<text>
							<span fg={THEME.warning}>warning: {warning}</span>
						</text>
					</box>
				) : null}
			</box>

			<box
				flexGrow={1}
				minHeight={0}
				marginTop={1}
				border
				borderColor={THEME.border}
				backgroundColor={THEME.panelBg}
				padding={1}
				flexDirection="column"
				overflow="hidden"
			>
				{rows.length === 0 ? (
					<text>
						<span fg={THEME.muted}>no active tasks</span>
					</text>
				) : (
					<box flexDirection="column" gap={0}>
						{visibleRows.map((row, index) => {
							const globalIndex = start + index;
							const selected = globalIndex === selectedIndex;
							const meta = metaBadge(row.task);
							// Fixed columns keep every cell painted so the live buffer
							// never leaves stale glyphs between refreshes.
							const idField = pad(row.task.id, idWidth);
							const marker = tree
								? `${treeMarker(row.hasChildren, row.isCollapsed)} `
								: "";
							const hiddenBadge = row.isCollapsed
								? ` (+${row.descendantCount})`
								: "";
							const titleBudget = Math.max(
								12,
								contentWidth -
									2 -
									idWidth -
									1 -
									row.prefix.length -
									marker.length -
									hiddenBadge.length -
									(meta.length + 2),
							);
							const title = titleWithEllipsis(row.task.title, titleBudget);
							return (
								<box
									key={row.task.id}
									backgroundColor={selected ? THEME.rowSelected : THEME.row}
								>
									<text>
										<span fg={STATUS_COLORS[row.task.status]}>
											{statusIcon(row.task.status)}{" "}
										</span>
										<span fg={THEME.focus}>{idField} </span>
										<span fg={THEME.dim}>{row.prefix}</span>
										<span fg={row.hasChildren ? THEME.muted : THEME.dim}>
											{marker}
										</span>
										<span fg={THEME.text}>{title}</span>
										<span fg={THEME.muted}>{hiddenBadge}</span>
										<span fg={THEME.dim}>{`  ${meta}`}</span>
									</text>
								</box>
							);
						})}
					</box>
				)}
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
						{`q/esc quit  r refresh  p ${paused ? "resume" : "pause"}  j/k scroll  g/G top/bottom${tree ? "  space/h/l fold  -/= fold all" : ""}`}
					</span>
				</text>
			</box>
		</box>
	);
}
