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
  created_at: string;
  last_seen_at: string | null;
};

export default async function UsersPage({
  searchParams,
}: {
  searchParams: Promise<{ q?: string }>;
}) {
  await requireAdminSession();
  const { q } = await searchParams;
  const needle = (q ?? "").trim();

  const query = needle ? `?q=${encodeURIComponent(needle)}` : "";
  const users = await apiGetJson<UserItem[]>(`/api/admin/users${query}`);

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold">Users</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Search users and apply moderation actions.
        </p>
      </div>

      <form className="flex gap-2" method="get">
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
