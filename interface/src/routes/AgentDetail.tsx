import { useMemo } from "react";
import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { api, type CortexEvent, type CronJobInfo, MEMORY_TYPES } from "@/api/client";
import type { ChannelLiveState } from "@/hooks/useChannelLiveState";
import { formatTimeAgo, formatDuration } from "@/lib/format";

interface AgentDetailProps {
	agentId: string;
	liveStates: Record<string, ChannelLiveState>;
}

export function AgentDetail({ agentId, liveStates }: AgentDetailProps) {
	const { data: agentsData } = useQuery({
		queryKey: ["agents"],
		queryFn: api.agents,
		refetchInterval: 30_000,
	});

	const { data: overviewData } = useQuery({
		queryKey: ["agent-overview", agentId],
		queryFn: () => api.agentOverview(agentId),
		refetchInterval: 15_000,
	});

	const { data: configData } = useQuery({
		queryKey: ["agent-config", agentId],
		queryFn: () => api.agentConfig(agentId),
		refetchInterval: 60_000,
	});

	const { data: identityData } = useQuery({
		queryKey: ["agent-identity", agentId],
		queryFn: () => api.agentIdentity(agentId),
		refetchInterval: 60_000,
	});

	const { data: channelsData } = useQuery({
		queryKey: ["channels"],
		queryFn: api.channels,
		refetchInterval: 10_000,
	});

	const agent = agentsData?.agents.find((a) => a.id === agentId);
	const agentChannels = useMemo(
		() => (channelsData?.channels ?? []).filter((c) => c.agent_id === agentId),
		[channelsData, agentId],
	);

	// Aggregate live activity for this agent
	const activity = useMemo(() => {
		let workers = 0;
		let branches = 0;
		let typing = 0;
		for (const channel of agentChannels) {
			const live = liveStates[channel.id];
			if (!live) continue;
			workers += Object.keys(live.workers).length;
			branches += Object.keys(live.branches).length;
			if (live.isTyping) typing++;
		}
		return { workers, branches, typing };
	}, [agentChannels, liveStates]);

	if (!agent) {
		return (
			<div className="flex h-full items-center justify-center">
				<p className="text-sm text-ink-faint">Agent not found: {agentId}</p>
			</div>
		);
	}

	return (
		<div className="h-full overflow-y-auto p-6">
			<div className="mx-auto flex max-w-5xl flex-col gap-6">
				{/* Live Activity */}
				<LiveActivitySection
					agentId={agentId}
					channelCount={agentChannels.length}
					workers={activity.workers}
					branches={activity.branches}
					typing={activity.typing}
					liveStates={liveStates}
					channels={agentChannels}
				/>

				{/* Memory Stats */}
				{overviewData && (
					<MemorySection
						agentId={agentId}
						total={overviewData.memory_total}
						counts={overviewData.memory_counts}
					/>
				)}

				{/* Model Routing */}
				{configData && <RoutingSection config={configData} />}

				{/* Identity Preview */}
				{identityData && <IdentitySection agentId={agentId} identity={identityData} />}

				{/* Cron Jobs */}
				{overviewData && overviewData.cron_jobs.length > 0 && (
					<CronSection agentId={agentId} jobs={overviewData.cron_jobs} />
				)}

				{/* Cortex Status */}
				{overviewData && (
					<CortexStatusSection
						agentId={agentId}
						lastBulletinAt={overviewData.last_bulletin_at}
						recentEvents={overviewData.recent_cortex_events}
					/>
				)}

				{/* Configuration */}
				<ConfigSection agent={agent} />
			</div>
		</div>
	);
}

// -- Section Components --

function SectionHeader({ title, action }: { title: string; action?: React.ReactNode }) {
	return (
		<div className="flex items-center justify-between">
			<h2 className="font-plex text-sm font-medium text-ink-dull">{title}</h2>
			{action}
		</div>
	);
}

function StatCard({ label, value, sub, color }: { label: string; value: string | number; sub?: string; color?: string }) {
	return (
		<div className="rounded-md bg-app-darkBox px-3 py-2">
			<span className="text-tiny text-ink-faint">{label}</span>
			<p className={`mt-0.5 text-lg font-medium tabular-nums ${color ?? "text-ink"}`}>
				{value}
			</p>
			{sub && <span className="text-tiny text-ink-faint">{sub}</span>}
		</div>
	);
}

// -- Live Activity --

function LiveActivitySection({
	agentId,
	channelCount,
	workers,
	branches,
	typing,
	liveStates,
	channels,
}: {
	agentId: string;
	channelCount: number;
	workers: number;
	branches: number;
	typing: number;
	liveStates: Record<string, ChannelLiveState>;
	channels: { id: string; display_name: string | null; platform: string }[];
}) {
	// Collect active workers/branches across channels for detail display
	const activeWorkers = useMemo(() => {
		const result: { channelName: string; worker: ChannelLiveState["workers"][string] }[] = [];
		for (const channel of channels) {
			const live = liveStates[channel.id];
			if (!live) continue;
			for (const worker of Object.values(live.workers)) {
				result.push({ channelName: channel.display_name ?? channel.id, worker });
			}
		}
		return result;
	}, [channels, liveStates]);

	const activeBranches = useMemo(() => {
		const result: { channelName: string; branch: ChannelLiveState["branches"][string] }[] = [];
		for (const channel of channels) {
			const live = liveStates[channel.id];
			if (!live) continue;
			for (const branch of Object.values(live.branches)) {
				result.push({ channelName: channel.display_name ?? channel.id, branch });
			}
		}
		return result;
	}, [channels, liveStates]);

	return (
		<section>
			<SectionHeader
				title="Activity"
				action={
					<Link
						to="/agents/$agentId/channels"
						params={{ agentId }}
						className="text-tiny text-accent hover:text-accent/80"
					>
						View channels
					</Link>
				}
			/>
			<div className="mt-3 grid grid-cols-2 gap-3 sm:grid-cols-4">
				<StatCard label="Channels" value={channelCount} />
				<StatCard
					label="Workers"
					value={workers}
					color={workers > 0 ? "text-amber-400" : undefined}
				/>
				<StatCard
					label="Branches"
					value={branches}
					color={branches > 0 ? "text-violet-400" : undefined}
				/>
				<StatCard
					label="Typing"
					value={typing}
					color={typing > 0 ? "text-accent" : undefined}
				/>
			</div>

			{/* Active process details */}
			{(activeWorkers.length > 0 || activeBranches.length > 0) && (
				<div className="mt-3 flex flex-col gap-1.5">
					{activeWorkers.map(({ channelName, worker }) => (
						<div
							key={worker.id}
							className="flex items-center gap-2 rounded-md bg-amber-500/10 px-3 py-2 text-sm"
						>
							<div className="h-1.5 w-1.5 animate-pulse rounded-full bg-amber-400" />
							<span className="font-medium text-amber-300">Worker</span>
							<span className="truncate text-ink-dull">{worker.task}</span>
							<span className="ml-auto text-tiny text-ink-faint">{channelName}</span>
							{worker.currentTool && (
								<span className="text-tiny text-amber-400/70">{worker.currentTool}</span>
							)}
						</div>
					))}
					{activeBranches.map(({ channelName, branch }) => (
						<div
							key={branch.id}
							className="flex items-center gap-2 rounded-md bg-violet-500/10 px-3 py-2 text-sm"
						>
							<div className="h-1.5 w-1.5 animate-pulse rounded-full bg-violet-400" />
							<span className="font-medium text-violet-300">Branch</span>
							<span className="truncate text-ink-dull">{branch.description}</span>
							<span className="ml-auto text-tiny text-ink-faint">{channelName}</span>
							{(branch.currentTool ?? branch.lastTool) && (
								<span className="text-tiny text-violet-400/70">
									{branch.currentTool ?? branch.lastTool}
								</span>
							)}
						</div>
					))}
				</div>
			)}
		</section>
	);
}

// -- Memory Stats --

const MEMORY_TYPE_COLORS: Record<string, string> = {
	fact: "bg-blue-500",
	preference: "bg-pink-500",
	decision: "bg-amber-500",
	identity: "bg-green-500",
	event: "bg-cyan-500",
	observation: "bg-purple-500",
	goal: "bg-orange-500",
	todo: "bg-red-500",
};

function MemorySection({
	agentId,
	total,
	counts,
}: {
	agentId: string;
	total: number;
	counts: Record<string, number>;
}) {
	return (
		<section>
			<SectionHeader
				title="Memory"
				action={
					<Link
						to="/agents/$agentId/memories"
						params={{ agentId }}
						className="text-tiny text-accent hover:text-accent/80"
					>
						View all
					</Link>
				}
			/>
			<div className="mt-3 rounded-md bg-app-darkBox p-3">
				<div className="flex items-baseline gap-2">
					<span className="text-2xl font-medium tabular-nums text-ink">{total}</span>
					<span className="text-sm text-ink-faint">total memories</span>
				</div>

				{/* Type breakdown bar */}
				{total > 0 && (
					<div className="mt-3 flex h-2 overflow-hidden rounded-full bg-app-box">
						{MEMORY_TYPES.map((type) => {
							const count = counts[type] ?? 0;
							if (count === 0) return null;
							const pct = (count / total) * 100;
							return (
								<div
									key={type}
									className={`${MEMORY_TYPE_COLORS[type] ?? "bg-gray-500"}`}
									style={{ width: `${pct}%` }}
									title={`${type}: ${count}`}
								/>
							);
						})}
					</div>
				)}

				{/* Legend */}
				<div className="mt-3 flex flex-wrap gap-x-4 gap-y-1">
					{MEMORY_TYPES.map((type) => {
						const count = counts[type] ?? 0;
						if (count === 0) return null;
						return (
							<div key={type} className="flex items-center gap-1.5 text-tiny">
								<div className={`h-2 w-2 rounded-full ${MEMORY_TYPE_COLORS[type] ?? "bg-gray-500"}`} />
								<span className="text-ink-dull">{type}</span>
								<span className="tabular-nums text-ink-faint">{count}</span>
							</div>
						);
					})}
				</div>
			</div>
		</section>
	);
}

// -- Model Routing --

function RoutingSection({ config }: { config: { routing: { channel: string; branch: string; worker: string; compactor: string; cortex: string } } }) {
	const models = [
		{ label: "Channel", model: config.routing.channel },
		{ label: "Branch", model: config.routing.branch },
		{ label: "Worker", model: config.routing.worker },
		{ label: "Compactor", model: config.routing.compactor },
		{ label: "Cortex", model: config.routing.cortex },
	];

	return (
		<section>
			<SectionHeader title="Model Routing" />
			<div className="mt-3 grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3">
				{models.map(({ label, model }) => (
					<div key={label} className="flex items-center justify-between rounded-md bg-app-darkBox px-3 py-2">
						<span className="text-tiny text-ink-faint">{label}</span>
						<span className="truncate pl-2 text-sm text-ink-dull" title={model}>
							{formatModelName(model)}
						</span>
					</div>
				))}
			</div>
		</section>
	);
}

/** Shorten model IDs like "anthropic/claude-sonnet-4-20250514" to "claude-sonnet-4" */
function formatModelName(model: string): string {
	const name = model.includes("/") ? model.split("/").pop()! : model;
	// Strip date suffixes like -20250514
	return name.replace(/-\d{8}$/, "");
}

// -- Identity Preview --

function IdentitySection({
	agentId,
	identity,
}: {
	agentId: string;
	identity: { soul: string | null; identity: string | null; user: string | null };
}) {
	const hasContent = identity.soul || identity.identity || identity.user;
	if (!hasContent) return null;

	const files = [
		{ label: "SOUL.md", content: identity.soul },
		{ label: "IDENTITY.md", content: identity.identity },
		{ label: "USER.md", content: identity.user },
	].filter((f) => f.content && f.content.trim().length > 0 && !f.content.startsWith("<!--"));

	if (files.length === 0) return null;

	return (
		<section>
			<SectionHeader
				title="Identity"
				action={
					<Link
						to="/agents/$agentId/config"
						params={{ agentId }}
						className="text-tiny text-accent hover:text-accent/80"
					>
						Edit
					</Link>
				}
			/>
			<div className="mt-3 grid grid-cols-1 gap-3 lg:grid-cols-2">
				{files.map(({ label, content }) => (
					<div key={label} className="rounded-md bg-app-darkBox p-3">
						<span className="text-tiny font-medium text-ink-faint">{label}</span>
						<p className="mt-1 line-clamp-4 whitespace-pre-wrap text-sm leading-relaxed text-ink-dull">
							{content!.trim()}
						</p>
					</div>
				))}
			</div>
		</section>
	);
}

// -- Cron Jobs --

function CronSection({ agentId, jobs }: { agentId: string; jobs: CronJobInfo[] }) {
	return (
		<section>
			<SectionHeader
				title="Cron Jobs"
				action={
					<Link
						to="/agents/$agentId/cron"
						params={{ agentId }}
						className="text-tiny text-accent hover:text-accent/80"
					>
						Manage
					</Link>
				}
			/>
			<div className="mt-3 flex flex-col gap-1.5">
				{jobs.map((job) => (
					<div
						key={job.id}
						className="flex items-center gap-3 rounded-md bg-app-darkBox px-3 py-2"
					>
						<div
							className={`h-2 w-2 rounded-full ${job.enabled ? "bg-green-500" : "bg-gray-500"}`}
							title={job.enabled ? "Enabled" : "Disabled"}
						/>
						<span className="min-w-0 flex-1 truncate text-sm text-ink-dull" title={job.prompt}>
							{job.prompt}
						</span>
						<span className="text-tiny tabular-nums text-ink-faint">
							every {formatDuration(job.interval_secs)}
						</span>
						{job.active_hours && (
							<span className="text-tiny text-ink-faint">
								{job.active_hours[0]}:00–{job.active_hours[1]}:00
							</span>
						)}
						<span className="text-tiny text-ink-faint">{job.delivery_target}</span>
					</div>
				))}
			</div>
		</section>
	);
}

// -- Cortex Status --

function CortexStatusSection({
	agentId,
	lastBulletinAt,
	recentEvents,
}: {
	agentId: string;
	lastBulletinAt: string | null;
	recentEvents: CortexEvent[];
}) {
	return (
		<section>
			<SectionHeader
				title="Cortex"
				action={
					<Link
						to="/agents/$agentId/cortex"
						params={{ agentId }}
						className="text-tiny text-accent hover:text-accent/80"
					>
						View all events
					</Link>
				}
			/>
			<div className="mt-3 rounded-md bg-app-darkBox p-3">
				<div className="flex items-center gap-4">
					<div>
						<span className="text-tiny text-ink-faint">Last Bulletin</span>
						<p className="mt-0.5 text-sm text-ink-dull">
							{lastBulletinAt ? formatTimeAgo(lastBulletinAt) : "No bulletins yet"}
						</p>
					</div>
				</div>

				{recentEvents.length > 0 && (
					<div className="mt-3 border-t border-app-line/50 pt-3">
						<span className="text-tiny text-ink-faint">Recent Events</span>
						<div className="mt-1.5 flex flex-col gap-1">
							{recentEvents.map((event) => (
								<div key={event.id} className="flex items-center gap-2 text-sm">
									<CortexEventBadge type={event.event_type} />
									<span className="min-w-0 flex-1 truncate text-ink-dull">{event.summary}</span>
									<span className="flex-shrink-0 text-tiny tabular-nums text-ink-faint">
										{formatTimeAgo(event.created_at)}
									</span>
								</div>
							))}
						</div>
					</div>
				)}
			</div>
		</section>
	);
}

const CORTEX_EVENT_COLORS: Record<string, string> = {
	bulletin_generated: "bg-green-500/20 text-green-400",
	bulletin_failed: "bg-red-500/20 text-red-400",
	maintenance_run: "bg-blue-500/20 text-blue-400",
	memory_merged: "bg-cyan-500/20 text-cyan-400",
	memory_decayed: "bg-yellow-500/20 text-yellow-400",
	memory_pruned: "bg-orange-500/20 text-orange-400",
	association_created: "bg-purple-500/20 text-purple-400",
	contradiction_flagged: "bg-red-500/20 text-red-400",
	worker_killed: "bg-red-500/20 text-red-400",
	branch_killed: "bg-red-500/20 text-red-400",
	circuit_breaker_tripped: "bg-amber-500/20 text-amber-400",
	observation_created: "bg-indigo-500/20 text-indigo-400",
	health_check: "bg-gray-500/20 text-gray-400",
};

function CortexEventBadge({ type }: { type: string }) {
	const color = CORTEX_EVENT_COLORS[type] ?? "bg-gray-500/20 text-gray-400";
	const label = type.replace(/_/g, " ");
	return (
		<span className={`flex-shrink-0 rounded px-1.5 py-0.5 text-tiny ${color}`}>
			{label}
		</span>
	);
}

// -- Configuration --

function ConfigSection({ agent }: { agent: { workspace: string; context_window: number; max_turns: number; max_concurrent_branches: number } }) {
	return (
		<section>
			<SectionHeader title="Configuration" />
			<div className="mt-3 grid grid-cols-2 gap-3 sm:grid-cols-4">
				<ConfigItem label="Workspace" value={agent.workspace} />
				<ConfigItem label="Context Window" value={agent.context_window.toLocaleString()} />
				<ConfigItem label="Max Turns" value={String(agent.max_turns)} />
				<ConfigItem label="Max Branches" value={String(agent.max_concurrent_branches)} />
			</div>
		</section>
	);
}

function ConfigItem({ label, value }: { label: string; value: string }) {
	return (
		<div className="rounded-md bg-app-darkBox px-3 py-2">
			<span className="text-tiny text-ink-faint">{label}</span>
			<p className="mt-0.5 truncate text-sm text-ink-dull" title={value}>{value}</p>
		</div>
	);
}
