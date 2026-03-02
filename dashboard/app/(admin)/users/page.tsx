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
    <div className="space-y-4">
      <div>
        <h1 className="text-xl font-semibold">Users</h1>
        <p className="text-sm text-muted-foreground">
          Search users and apply moderation actions.
        </p>
      </div>

      <form className="flex gap-2" method="get">
        <input
          className="w-full rounded-md border bg-background px-3 py-2"
          defaultValue={needle}
          name="q"
          placeholder="Search by username or email"
          type="text"
        />
        <button className="rounded-md border px-4 py-2" type="submit">
          Search
        </button>
      </form>

      <UsersTable users={users} />
    </div>
  );
}
