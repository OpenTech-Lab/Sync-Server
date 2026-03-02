"use client";

import { useRouter } from "next/navigation";

export function LogoutButton() {
  const router = useRouter();

  async function logout() {
    await fetch("/api/session/logout", { method: "POST" });
    router.push("/login");
    router.refresh();
  }

  return (
    <button
      className="rounded-md border px-3 py-2 text-sm hover:bg-accent"
      onClick={logout}
      type="button"
    >
      Sign out
    </button>
  );
}
