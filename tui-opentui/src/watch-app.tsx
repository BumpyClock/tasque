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
import { THEME, clampIndex, pad, statusIcon, visibleRange } from "./tui-helpers";
import {
	buildWatchRows,
	filterLabel,
	metaBadge,
	readWatchTreeFlag,
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
		() => buildWatchRows(snapshot.tasks, tree),
		[snapshot.tasks, tree],
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
	// Reserve every non-list row so the selected item never clips off the bottom:
	//   root padding 2 + header (border 2 + 3 lines, +1 when a warning shows)
	//   + list marginTop 1 + list (border 2 + padding 2) + footer marginTop 1
	//   + footer (border 2 + 1 line) = 16, or 17 with the warning line.
	const chromeRows = 16 + (warning ? 1 : 0);
	const rowBudget = Math.max(3, dimensions.height - chromeRows);
	const [start, end] = visibleRange(selectedIndex, rows.length, rowBudget);
	const visibleRows = rows.slice(start, end);

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
							const idField = pad(row.task.id, 9);
							const titleBudget = Math.max(
								12,
								contentWidth - 2 - 9 - 1 - row.prefix.length - (meta.length + 2),
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
										<span fg={THEME.text}>{title}</span>
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
						{`q/esc quit  r refresh  p ${paused ? "resume" : "pause"}  j/k or up/down scroll  g/G top/bottom`}
					</span>
				</text>
			</box>
		</box>
	);
}
