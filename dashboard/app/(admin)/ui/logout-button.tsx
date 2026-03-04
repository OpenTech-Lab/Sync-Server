"use client";

import { LogOut } from "lucide-react";
import { useRouter } from "next/navigation";

import { Button } from "@/components/ui/button";

export function LogoutButton() {
  const router = useRouter();

  async function logout() {
    const response = await fetch("./api/session/logout", { method: "POST" });
    if (!response.ok) {
      return;
    }

    router.push("./login");
    router.refresh();
  }

  return (
    <Button onClick={logout} type="button" variant="outline">
      <LogOut className="size-4" />
      Sign out
    </Button>
  );
}
