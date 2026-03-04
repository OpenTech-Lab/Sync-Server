import { Search } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Card,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
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
    <div className="space-y-4">
      <Card className="py-0">
        <CardHeader>
          <CardTitle className="text-2xl">Users</CardTitle>
          <CardDescription>
            Search users and apply moderation actions.
          </CardDescription>
        </CardHeader>
      </Card>

      <form className="flex flex-col gap-2 sm:flex-row" method="get">
        <Input
          defaultValue={needle}
          name="q"
          placeholder="Search by username or email"
          type="text"
        />
        <Button type="submit" variant="secondary">
          <Search className="size-4" />
          Search
        </Button>
      </form>

      <UsersTable users={users} />
    </div>
  );
}
