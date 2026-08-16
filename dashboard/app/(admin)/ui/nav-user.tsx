"use client";

import { ChevronsUpDown, LogOut, Monitor, Moon, ShieldCheck, Sun } from "lucide-react";
import { useTheme } from "next-themes";
import { useRouter } from "next/navigation";

import { apiUrl } from "@/lib/client-api";
import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import { Badge } from "@/components/ui/badge";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar";

export function NavUser({ username, role }: { username: string; role: string }) {
  const { setTheme } = useTheme();
  const router = useRouter();
  const initial = username.charAt(0).toUpperCase() || "A";

  async function logout() {
    const response = await fetch(apiUrl("/api/session/logout"), { method: "POST" });
    if (!response.ok) {
      return;
    }

    router.push("/");
    router.refresh();
  }

  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <SidebarMenuButton
              className="data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground"
              size="lg"
            >
              <Avatar className="size-8 rounded-full">
                <AvatarFallback className="rounded-full bg-gradient-to-br from-emerald-300 to-cyan-500 text-xs font-semibold text-white">
                  {initial}
                </AvatarFallback>
              </Avatar>
              <div className="grid flex-1 text-left text-sm leading-tight">
                <span className="truncate font-medium">{username}</span>
                <span className="truncate text-xs text-muted-foreground capitalize">{role}</span>
              </div>
              <ChevronsUpDown className="ml-auto size-4 text-muted-foreground" />
            </SidebarMenuButton>
          </DropdownMenuTrigger>
          <DropdownMenuContent
            align="end"
            className="w-(--radix-dropdown-menu-trigger-width) min-w-56"
            side="top"
            sideOffset={4}
          >
            <DropdownMenuLabel className="p-0 font-normal">
              <div className="flex items-center gap-2 px-1 py-1.5">
                <Avatar className="size-8 rounded-full">
                  <AvatarFallback className="rounded-full bg-gradient-to-br from-emerald-300 to-cyan-500 text-xs font-semibold text-white">
                    {initial}
                  </AvatarFallback>
                </Avatar>
                <div className="grid flex-1 text-left text-sm leading-tight">
                  <span className="truncate font-medium">{username}</span>
                  <Badge className="mt-0.5 w-fit text-[10px] capitalize" variant="outline">
                    {role}
                  </Badge>
                </div>
              </div>
              <div className="flex items-center gap-1.5 px-1 pt-2 text-[10px] text-muted-foreground">
                <ShieldCheck aria-hidden="true" className="size-3.5 text-emerald-600 dark:text-emerald-400" />
                <span>Admin session active</span>
              </div>
            </DropdownMenuLabel>
            <DropdownMenuSeparator />
            <DropdownMenuSub>
              <DropdownMenuSubTrigger>
                <Sun className="size-3.5" />
                Theme
              </DropdownMenuSubTrigger>
              <DropdownMenuSubContent>
                <DropdownMenuItem onClick={() => setTheme("light")}>
                  <Sun className="size-3.5" />
                  Light
                </DropdownMenuItem>
                <DropdownMenuItem onClick={() => setTheme("dark")}>
                  <Moon className="size-3.5" />
                  Dark
                </DropdownMenuItem>
                <DropdownMenuItem onClick={() => setTheme("system")}>
                  <Monitor className="size-3.5" />
                  System
                </DropdownMenuItem>
              </DropdownMenuSubContent>
            </DropdownMenuSub>
            <DropdownMenuSeparator />
            <DropdownMenuItem onClick={logout}>
              <LogOut className="size-3.5" />
              Sign out
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  );
}
