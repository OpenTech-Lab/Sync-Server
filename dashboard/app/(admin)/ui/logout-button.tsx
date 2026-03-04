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
    <Button onClick={logout} size="sm" type="button" variant="ghost" className="gap-1.5 text-muted-foreground hover:text-foreground">
      <LogOut className="size-3.5" />
      <span className="hidden sm:inline">Sign out</span>
    </Button>
  );
}
