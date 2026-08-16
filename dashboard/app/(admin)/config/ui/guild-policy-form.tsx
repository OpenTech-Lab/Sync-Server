"use client";

import { useRouter } from "next/navigation";
import { useMemo, useState } from "react";

import { apiUrl } from "@/lib/client-api";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";

type GuildPolicyConfig = {
  enforcement: {
    enabled: boolean;
    outbound_messages_enabled: boolean;
    friend_adds_enabled: boolean;
    attachment_sends_enabled: boolean;
  };
  daily_counter_retention_days: number;
  score_event_retention_days: number;
  level_policies: Array<{
    level: number;
    min_active_days: number;
    max_active_days: number | null;
    daily_outbound_messages_limit: number | null;
    daily_friend_add_limit: number | null;
    daily_attachment_send_limit: number | null;
  }>;
  rank_policies: Array<{
    rank: string;
    min_score: number;
    max_score: number | null;
    daily_outbound_messages_limit_multiplier_percent: number | null;
    daily_friend_add_limit_multiplier_percent: number | null;
    daily_attachment_send_limit_multiplier_percent: number | null;
    overrides_level_limits: boolean;
  }>;
  community_upvote_daily_cap: number;
  safe_attachment_types: string[];
};

type LevelPolicy = GuildPolicyConfig["level_policies"][number];
type RankPolicy = GuildPolicyConfig["rank_policies"][number];

type GuildHistoryPruneResult = {
  daily_counter_retention_days: number;
  score_event_retention_days: number;
  pruned_before_day: string;
  pruned_before_timestamp: string;
  daily_action_counters_deleted: number;
  guild_score_events_deleted: number;
};

function formatPolicy(policy: GuildPolicyConfig) {
  return JSON.stringify(policy, null, 2);
}

/** Nullable number → display string */
function nn(v: number | null): string {
  return v === null ? "" : String(v);
}

/** Input string → nullable number */
function pnn(s: string): number | null {
  const t = s.trim();
  return t === "" ? null : Number(t);
}

function SwitchRow({
  id,
  label,
  checked,
  onCheckedChange,
}: {
  id: string;
  label: string;
  checked: boolean;
  onCheckedChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between gap-4 rounded-lg border p-3">
      <Label className="cursor-pointer text-sm" htmlFor={id}>
        {label}
      </Label>
      <Switch checked={checked} id={id} onCheckedChange={onCheckedChange} />
    </div>
  );
}

export function GuildPolicyForm({ policy }: { policy: GuildPolicyConfig }) {
  const router = useRouter();
  const [draft, setDraft] = useState(() => formatPolicy(policy));
  const [saving, setSaving] = useState(false);
  const [pruning, setPruning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [attachmentInput, setAttachmentInput] = useState("");

  const parseResult = useMemo(() => {
    try {
      return { value: JSON.parse(draft) as GuildPolicyConfig, error: null };
    } catch (e) {
      return { value: null, error: e instanceof Error ? e.message : "Invalid JSON" };
    }
  }, [draft]);

  /** Apply an immutable update to the parsed policy and sync back to draft. */
  function update(updater: (p: GuildPolicyConfig) => GuildPolicyConfig) {
    const current = parseResult.value;
    if (!current) return;
    setDraft(formatPolicy(updater(current)));
  }

  async function onSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);
    setNotice(null);

    if (parseResult.error || !parseResult.value) {
      setError(parseResult.error ?? "Guild policy JSON is invalid");
      return;
    }

    setSaving(true);
    const response = await fetch(apiUrl("/api/admin/guild-policy"), {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(parseResult.value),
    });

    if (!response.ok) {
      const body = (await response.json().catch(() => null)) as { error?: string } | null;
      setError(body?.error ?? "Failed to save guild policy");
      setSaving(false);
      return;
    }

    const nextPolicy = (await response.json()) as GuildPolicyConfig;
    setDraft(formatPolicy(nextPolicy));
    setNotice("Guild policy updated.");
    setSaving(false);
    router.refresh();
  }

  async function pruneHistory() {
    if (!confirm("Prune guild counters and score history using the current retention windows?")) {
      return;
    }

    setError(null);
    setNotice(null);
    setPruning(true);

    const response = await fetch(apiUrl("/api/admin/guild-policy/prune-history"), { method: "POST" });

    if (!response.ok) {
      const body = (await response.json().catch(() => null)) as { error?: string } | null;
      setError(body?.error ?? "Failed to prune guild history");
      setPruning(false);
      return;
    }

    const result = (await response.json()) as GuildHistoryPruneResult;
    setNotice(
      `Pruned ${result.daily_action_counters_deleted} daily counters and ${result.guild_score_events_deleted} score events.`,
    );
    setPruning(false);
    router.refresh();
  }

  function resetDraft() {
    setDraft(formatPolicy(policy));
    setError(null);
    setNotice(null);
  }

  function formatDraft() {
    if (!parseResult.value) {
      setError(parseResult.error ?? "Guild policy JSON is invalid");
      return;
    }
    setDraft(formatPolicy(parseResult.value));
    setError(null);
  }

  // ── Level policy helpers ─────────────────────────────────────────────────

  function updateLevelPolicy(idx: number, field: keyof LevelPolicy, raw: string) {
    update((p) => ({
      ...p,
      level_policies: p.level_policies.map((row, i) => {
        if (i !== idx) return row;
        const val =
          field === "level" || field === "min_active_days" ? Number(raw) || 0 : pnn(raw);
        return { ...row, [field]: val };
      }),
    }));
  }

  function addLevelPolicy() {
    update((p) => ({
      ...p,
      level_policies: [
        ...p.level_policies,
        {
          level: p.level_policies.length + 1,
          min_active_days: 0,
          max_active_days: null,
          daily_outbound_messages_limit: null,
          daily_friend_add_limit: null,
          daily_attachment_send_limit: null,
        },
      ],
    }));
  }

  function removeLevelPolicy(idx: number) {
    update((p) => ({
      ...p,
      level_policies: p.level_policies.filter((_, i) => i !== idx),
    }));
  }

  // ── Rank policy helpers ──────────────────────────────────────────────────

  function updateRankPolicy(idx: number, field: keyof RankPolicy, raw: string | boolean) {
    update((p) => ({
      ...p,
      rank_policies: p.rank_policies.map((row, i) => {
        if (i !== idx) return row;
        let val: string | number | boolean | null;
        if (typeof raw === "boolean") {
          val = raw;
        } else if (field === "rank") {
          val = raw;
        } else if (field === "min_score") {
          val = Number(raw) || 0;
        } else {
          val = pnn(raw);
        }
        return { ...row, [field]: val };
      }),
    }));
  }

  function addRankPolicy() {
    update((p) => ({
      ...p,
      rank_policies: [
        ...p.rank_policies,
        {
          rank: "",
          min_score: 0,
          max_score: null,
          daily_outbound_messages_limit_multiplier_percent: null,
          daily_friend_add_limit_multiplier_percent: null,
          daily_attachment_send_limit_multiplier_percent: null,
          overrides_level_limits: false,
        },
      ],
    }));
  }

  function removeRankPolicy(idx: number) {
    update((p) => ({
      ...p,
      rank_policies: p.rank_policies.filter((_, i) => i !== idx),
    }));
  }

  // ── Attachment type helpers ──────────────────────────────────────────────

  function addAttachmentType() {
    const trimmed = attachmentInput.trim();
    if (!trimmed) return;
    update((p) => {
      if (p.safe_attachment_types.includes(trimmed)) return p;
      return { ...p, safe_attachment_types: [...p.safe_attachment_types, trimmed] };
    });
    setAttachmentInput("");
  }

  function removeAttachmentType(type: string) {
    update((p) => ({
      ...p,
      safe_attachment_types: p.safe_attachment_types.filter((t) => t !== type),
    }));
  }

  const p = parseResult.value;

  return (
    <form className="space-y-6" onSubmit={onSubmit}>
      <Separator />

      <section className="space-y-4">
        <div>
          <h2 className="text-xl font-semibold">Guild policy</h2>
          <p className="mt-1 text-sm text-muted-foreground">
            Manage guild caps, progression thresholds, enforcement switches, and
            attachment allowlists. The server validates and normalizes policy
            ranges before saving.
          </p>
        </div>

        <Tabs defaultValue="visual">
          <TabsList>
            <TabsTrigger value="visual">Visual editor</TabsTrigger>
            <TabsTrigger value="json">JSON</TabsTrigger>
          </TabsList>

          {/* ── Visual editor ────────────────────────────────────────────── */}
          <TabsContent value="visual" className="space-y-5 pt-4">
            {!p ? (
              <Alert variant="destructive">
                <AlertDescription>
                  JSON is invalid — switch to the JSON tab to fix the syntax
                  error before editing here.
                </AlertDescription>
              </Alert>
            ) : (
              <>
                {/* Enforcement */}
                <div className="space-y-3 rounded-lg border p-4">
                  <p className="text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
                    Enforcement
                  </p>
                  <div className="grid gap-2 sm:grid-cols-2">
                    <SwitchRow
                      checked={p.enforcement.enabled}
                      id="enf-global"
                      label="Global guild enforcement"
                      onCheckedChange={(v) =>
                        update((x) => ({
                          ...x,
                          enforcement: { ...x.enforcement, enabled: v },
                        }))
                      }
                    />
                    <SwitchRow
                      checked={p.enforcement.outbound_messages_enabled}
                      id="enf-messages"
                      label="Outbound messages"
                      onCheckedChange={(v) =>
                        update((x) => ({
                          ...x,
                          enforcement: { ...x.enforcement, outbound_messages_enabled: v },
                        }))
                      }
                    />
                    <SwitchRow
                      checked={p.enforcement.friend_adds_enabled}
                      id="enf-friends"
                      label="Friend adds"
                      onCheckedChange={(v) =>
                        update((x) => ({
                          ...x,
                          enforcement: { ...x.enforcement, friend_adds_enabled: v },
                        }))
                      }
                    />
                    <SwitchRow
                      checked={p.enforcement.attachment_sends_enabled}
                      id="enf-attachments"
                      label="Attachment sends"
                      onCheckedChange={(v) =>
                        update((x) => ({
                          ...x,
                          enforcement: { ...x.enforcement, attachment_sends_enabled: v },
                        }))
                      }
                    />
                  </div>
                </div>

                {/* Retention & caps */}
                <div className="space-y-3 rounded-lg border p-4">
                  <p className="text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
                    Retention &amp; caps
                  </p>
                  <div className="grid gap-4 sm:grid-cols-3">
                    <div className="space-y-2">
                      <Label htmlFor="counter-retention">Counter retention (days)</Label>
                      <Input
                        id="counter-retention"
                        min={1}
                        onChange={(e) =>
                          update((x) => ({
                            ...x,
                            daily_counter_retention_days: Number(e.target.value) || 1,
                          }))
                        }
                        type="number"
                        value={p.daily_counter_retention_days}
                      />
                    </div>
                    <div className="space-y-2">
                      <Label htmlFor="score-retention">Score retention (days)</Label>
                      <Input
                        id="score-retention"
                        min={1}
                        onChange={(e) =>
                          update((x) => ({
                            ...x,
                            score_event_retention_days: Number(e.target.value) || 1,
                          }))
                        }
                        type="number"
                        value={p.score_event_retention_days}
                      />
                    </div>
                    <div className="space-y-2">
                      <Label htmlFor="upvote-cap">Upvote daily cap</Label>
                      <Input
                        id="upvote-cap"
                        min={0}
                        onChange={(e) =>
                          update((x) => ({
                            ...x,
                            community_upvote_daily_cap: Number(e.target.value) || 0,
                          }))
                        }
                        type="number"
                        value={p.community_upvote_daily_cap}
                      />
                    </div>
                  </div>
                </div>

                {/* Safe attachment types */}
                <div className="space-y-3 rounded-lg border p-4">
                  <p className="text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
                    Safe attachment types
                  </p>
                  <div className="flex flex-wrap gap-2">
                    {p.safe_attachment_types.length === 0 ? (
                      <span className="text-sm text-muted-foreground">None allowed.</span>
                    ) : null}
                    {p.safe_attachment_types.map((type) => (
                      <Badge className="gap-1 pl-2.5 pr-1" key={type} variant="secondary">
                        {type}
                        <button
                          aria-label={`Remove ${type}`}
                          className="ml-0.5 rounded-sm opacity-60 hover:opacity-100"
                          onClick={() => removeAttachmentType(type)}
                          type="button"
                        >
                          ×
                        </button>
                      </Badge>
                    ))}
                  </div>
                  <div className="flex gap-2">
                    <Input
                      className="max-w-[16rem]"
                      onChange={(e) => setAttachmentInput(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          e.preventDefault();
                          addAttachmentType();
                        }
                      }}
                      placeholder="image/png"
                      type="text"
                      value={attachmentInput}
                    />
                    <Button
                      disabled={!attachmentInput.trim()}
                      onClick={addAttachmentType}
                      size="sm"
                      type="button"
                      variant="outline"
                    >
                      Add
                    </Button>
                  </div>
                </div>

                {/* Level policies */}
                <div className="space-y-3">
                  <p className="text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
                    Level policies
                  </p>
                  <div className="overflow-x-auto rounded-lg border">
                    <Table>
                      <TableHeader className="bg-muted/30">
                        <TableRow>
                          <TableHead className="w-20">Level</TableHead>
                          <TableHead>Min days</TableHead>
                          <TableHead>Max days</TableHead>
                          <TableHead>Msg/day</TableHead>
                          <TableHead>Friends/day</TableHead>
                          <TableHead>Attach/day</TableHead>
                          <TableHead className="w-10" />
                        </TableRow>
                      </TableHeader>
                      <TableBody>
                        {p.level_policies.length === 0 ? (
                          <TableRow>
                            <TableCell
                              className="py-6 text-center text-sm text-muted-foreground"
                              colSpan={7}
                            >
                              No level policies.
                            </TableCell>
                          </TableRow>
                        ) : null}
                        {p.level_policies.map((row, idx) => (
                          <TableRow key={idx}>
                            <TableCell>
                              <Input
                                className="h-7 w-16 text-xs"
                                min={1}
                                onChange={(e) =>
                                  updateLevelPolicy(idx, "level", e.target.value)
                                }
                                type="number"
                                value={row.level}
                              />
                            </TableCell>
                            <TableCell>
                              <Input
                                className="h-7 w-20 text-xs"
                                min={0}
                                onChange={(e) =>
                                  updateLevelPolicy(idx, "min_active_days", e.target.value)
                                }
                                type="number"
                                value={row.min_active_days}
                              />
                            </TableCell>
                            <TableCell>
                              <Input
                                className="h-7 w-20 text-xs"
                                min={0}
                                onChange={(e) =>
                                  updateLevelPolicy(idx, "max_active_days", e.target.value)
                                }
                                placeholder="∞"
                                type="number"
                                value={nn(row.max_active_days)}
                              />
                            </TableCell>
                            <TableCell>
                              <Input
                                className="h-7 w-20 text-xs"
                                min={0}
                                onChange={(e) =>
                                  updateLevelPolicy(
                                    idx,
                                    "daily_outbound_messages_limit",
                                    e.target.value,
                                  )
                                }
                                placeholder="∞"
                                type="number"
                                value={nn(row.daily_outbound_messages_limit)}
                              />
                            </TableCell>
                            <TableCell>
                              <Input
                                className="h-7 w-20 text-xs"
                                min={0}
                                onChange={(e) =>
                                  updateLevelPolicy(
                                    idx,
                                    "daily_friend_add_limit",
                                    e.target.value,
                                  )
                                }
                                placeholder="∞"
                                type="number"
                                value={nn(row.daily_friend_add_limit)}
                              />
                            </TableCell>
                            <TableCell>
                              <Input
                                className="h-7 w-20 text-xs"
                                min={0}
                                onChange={(e) =>
                                  updateLevelPolicy(
                                    idx,
                                    "daily_attachment_send_limit",
                                    e.target.value,
                                  )
                                }
                                placeholder="∞"
                                type="number"
                                value={nn(row.daily_attachment_send_limit)}
                              />
                            </TableCell>
                            <TableCell>
                              <Button
                                className="h-7 w-7 text-destructive hover:text-destructive"
                                onClick={() => removeLevelPolicy(idx)}
                                size="icon"
                                type="button"
                                variant="ghost"
                              >
                                ×
                              </Button>
                            </TableCell>
                          </TableRow>
                        ))}
                      </TableBody>
                    </Table>
                  </div>
                  <Button
                    className="text-xs"
                    onClick={addLevelPolicy}
                    size="sm"
                    type="button"
                    variant="outline"
                  >
                    + Add level
                  </Button>
                </div>

                {/* Rank policies */}
                <div className="space-y-3">
                  <p className="text-xs font-semibold tracking-widest text-muted-foreground/70 uppercase">
                    Rank policies
                  </p>
                  <div className="overflow-x-auto rounded-lg border">
                    <Table>
                      <TableHeader className="bg-muted/30">
                        <TableRow>
                          <TableHead>Rank</TableHead>
                          <TableHead>Min score</TableHead>
                          <TableHead>Max score</TableHead>
                          <TableHead>Msg %</TableHead>
                          <TableHead>Friend %</TableHead>
                          <TableHead>Attach %</TableHead>
                          <TableHead>Override</TableHead>
                          <TableHead className="w-10" />
                        </TableRow>
                      </TableHeader>
                      <TableBody>
                        {p.rank_policies.length === 0 ? (
                          <TableRow>
                            <TableCell
                              className="py-6 text-center text-sm text-muted-foreground"
                              colSpan={8}
                            >
                              No rank policies.
                            </TableCell>
                          </TableRow>
                        ) : null}
                        {p.rank_policies.map((row, idx) => (
                          <TableRow key={idx}>
                            <TableCell>
                              <Input
                                className="h-7 w-24 text-xs"
                                onChange={(e) =>
                                  updateRankPolicy(idx, "rank", e.target.value)
                                }
                                placeholder="bronze"
                                type="text"
                                value={row.rank}
                              />
                            </TableCell>
                            <TableCell>
                              <Input
                                className="h-7 w-20 text-xs"
                                min={0}
                                onChange={(e) =>
                                  updateRankPolicy(idx, "min_score", e.target.value)
                                }
                                type="number"
                                value={row.min_score}
                              />
                            </TableCell>
                            <TableCell>
                              <Input
                                className="h-7 w-20 text-xs"
                                min={0}
                                onChange={(e) =>
                                  updateRankPolicy(idx, "max_score", e.target.value)
                                }
                                placeholder="∞"
                                type="number"
                                value={nn(row.max_score)}
                              />
                            </TableCell>
                            <TableCell>
                              <Input
                                className="h-7 w-20 text-xs"
                                min={0}
                                onChange={(e) =>
                                  updateRankPolicy(
                                    idx,
                                    "daily_outbound_messages_limit_multiplier_percent",
                                    e.target.value,
                                  )
                                }
                                placeholder="—"
                                type="number"
                                value={nn(
                                  row.daily_outbound_messages_limit_multiplier_percent,
                                )}
                              />
                            </TableCell>
                            <TableCell>
                              <Input
                                className="h-7 w-20 text-xs"
                                min={0}
                                onChange={(e) =>
                                  updateRankPolicy(
                                    idx,
                                    "daily_friend_add_limit_multiplier_percent",
                                    e.target.value,
                                  )
                                }
                                placeholder="—"
                                type="number"
                                value={nn(row.daily_friend_add_limit_multiplier_percent)}
                              />
                            </TableCell>
                            <TableCell>
                              <Input
                                className="h-7 w-20 text-xs"
                                min={0}
                                onChange={(e) =>
                                  updateRankPolicy(
                                    idx,
                                    "daily_attachment_send_limit_multiplier_percent",
                                    e.target.value,
                                  )
                                }
                                placeholder="—"
                                type="number"
                                value={nn(
                                  row.daily_attachment_send_limit_multiplier_percent,
                                )}
                              />
                            </TableCell>
                            <TableCell>
                              <Switch
                                checked={row.overrides_level_limits}
                                onCheckedChange={(v) =>
                                  updateRankPolicy(idx, "overrides_level_limits", v)
                                }
                              />
                            </TableCell>
                            <TableCell>
                              <Button
                                className="h-7 w-7 text-destructive hover:text-destructive"
                                onClick={() => removeRankPolicy(idx)}
                                size="icon"
                                type="button"
                                variant="ghost"
                              >
                                ×
                              </Button>
                            </TableCell>
                          </TableRow>
                        ))}
                      </TableBody>
                    </Table>
                  </div>
                  <Button
                    className="text-xs"
                    onClick={addRankPolicy}
                    size="sm"
                    type="button"
                    variant="outline"
                  >
                    + Add rank
                  </Button>
                </div>
              </>
            )}
          </TabsContent>

          {/* ── JSON tab ─────────────────────────────────────────────────── */}
          <TabsContent value="json" className="space-y-3 pt-4">
            <div className="space-y-2">
              <Label htmlFor="guild-policy-json">Guild policy JSON</Label>
              <Textarea
                className="min-h-[28rem] font-mono text-xs"
                id="guild-policy-json"
                onChange={(event) => setDraft(event.target.value)}
                value={draft}
              />
              <p className="text-xs text-muted-foreground">
                Edit the full policy directly to change caps, thresholds, or safe
                attachment types. Save will fail if ranges overlap or required
                fields are invalid.
              </p>
            </div>
            <Button onClick={formatDraft} size="sm" type="button" variant="outline">
              Format JSON
            </Button>
          </TabsContent>
        </Tabs>
      </section>

      {parseResult.error ? (
        <Alert variant="destructive">
          <AlertDescription>JSON parse error: {parseResult.error}</AlertDescription>
        </Alert>
      ) : null}

      {error ? (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      ) : null}

      {notice ? (
        <Alert>
          <AlertDescription>{notice}</AlertDescription>
        </Alert>
      ) : null}

      <div className="flex flex-wrap items-center gap-2">
        <Button onClick={resetDraft} type="button" variant="outline">
          Reset draft
        </Button>
        <Button
          disabled={pruning || saving}
          onClick={pruneHistory}
          type="button"
          variant="destructive"
        >
          {pruning ? "Pruning…" : "Prune retained history"}
        </Button>
        <div className="flex-1" />
        <Button disabled={saving || !!parseResult.error} type="submit">
          {saving ? "Saving…" : "Save guild policy"}
        </Button>
      </div>
    </form>
  );
}
