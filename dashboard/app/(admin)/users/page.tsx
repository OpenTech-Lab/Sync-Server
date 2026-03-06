import { Search } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { apiGetJson } from "@/lib/server-api";
import { requireAdminSession } from "@/lib/session";

import { UsersTable } from "./ui/users-table";

type UserItem = {
  id: string;
  username: string;
  email: string;
  role: string;
  is_active: boolean;
  is_approved: boolean;
  created_at: string;
  last_seen_at: string | null;
};

export default async function UsersPage({
  searchParams,
}: {
  searchParams: Promise<{ q?: string; pending?: string }>;
}) {
  await requireAdminSession();
  const { q, pending } = await searchParams;
  const needle = (q ?? "").trim();
  const showPending = pending === "1";

  const query = needle ? `?q=${encodeURIComponent(needle)}` : "";
  const allUsers = await apiGetJson<UserItem[]>(`/api/admin/users${query}`);
  const pendingCount = allUsers.filter((u) => !u.is_approved).length;
  const users = showPending ? allUsers.filter((u) => !u.is_approved) : allUsers;

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold">Users</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Search users and apply moderation actions.
        </p>
      </div>

      <div className="flex items-center gap-3">
        <a
          href="/users"
          className={`text-sm font-medium pb-1 border-b-2 transition-colors ${!showPending ? "border-foreground" : "border-transparent text-muted-foreground hover:text-foreground"}`}
        >
          All users
        </a>
        <a
          href="/users?pending=1"
          className={`flex items-center gap-1.5 text-sm font-medium pb-1 border-b-2 transition-colors ${showPending ? "border-foreground" : "border-transparent text-muted-foreground hover:text-foreground"}`}
        >
          Pending approval
          {pendingCount > 0 && (
            <span className="inline-flex items-center justify-center rounded-full bg-amber-100 px-1.5 py-0.5 text-xs font-semibold text-amber-800">
              {pendingCount}
            </span>
          )}
        </a>
      </div>

      <form className="flex gap-2" method="get">
        {showPending && <input type="hidden" name="pending" value="1" />}
        <div className="relative flex-1">
          <Search className="absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            className="pl-8"
            defaultValue={needle}
            name="q"
            placeholder="Search by username or email…"
            type="text"
          />
        </div>
        <Button type="submit" variant="secondary">
          Search
        </Button>
      </form>

      <UsersTable users={users} />
    </div>
  );
}
