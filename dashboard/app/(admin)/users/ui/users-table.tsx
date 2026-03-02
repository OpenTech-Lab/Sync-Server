"use client";

import { useRouter } from "next/navigation";
import { useState } from "react";

type UserItem = {
  id: string;
  username: string;
  email: string;
  role: string;
  is_active: boolean;
  created_at: string;
  last_seen_at: string | null;
};

export function UsersTable({ users }: { users: UserItem[] }) {
  const router = useRouter();
  const [workingUserId, setWorkingUserId] = useState<string | null>(null);

  async function toggleStatus(user: UserItem) {
    setWorkingUserId(user.id);
    const action = user.is_active ? "suspend" : "activate";
    await fetch(`/api/admin/users/${user.id}/${action}`, { method: "POST" });
    setWorkingUserId(null);
    router.refresh();
  }

  return (
    <div className="overflow-x-auto rounded-lg border bg-background">
      <table className="min-w-full text-sm">
        <thead className="border-b bg-muted/40 text-left">
          <tr>
            <th className="px-3 py-2 font-medium">User</th>
            <th className="px-3 py-2 font-medium">Role</th>
            <th className="px-3 py-2 font-medium">Status</th>
            <th className="px-3 py-2 font-medium">Last Seen</th>
            <th className="px-3 py-2 font-medium">Actions</th>
          </tr>
        </thead>
        <tbody>
          {users.map((user) => (
            <tr className="border-b" key={user.id}>
              <td className="px-3 py-2">
                <p className="font-medium">{user.username}</p>
                <p className="text-muted-foreground">{user.email}</p>
              </td>
              <td className="px-3 py-2">{user.role}</td>
              <td className="px-3 py-2">{user.is_active ? "active" : "suspended"}</td>
              <td className="px-3 py-2 text-muted-foreground">
                {user.last_seen_at
                  ? new Date(user.last_seen_at).toLocaleString()
                  : "never"}
              </td>
              <td className="px-3 py-2">
                <button
                  className="rounded-md border px-3 py-1 disabled:opacity-70"
                  disabled={workingUserId === user.id}
                  onClick={() => toggleStatus(user)}
                  type="button"
                >
                  {workingUserId === user.id
                    ? "Updating..."
                    : user.is_active
                      ? "Suspend"
                      : "Activate"}
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
