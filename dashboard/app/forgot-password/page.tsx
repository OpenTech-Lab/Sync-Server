import { AuthShell } from "@/app/ui/auth-shell";

import { ForgotPasswordForm } from "./ui/forgot-password-form";

export default function ForgotPasswordPage() {
  return (
    <AuthShell
      title="Forgot password"
      description="Enter your admin email and we will send a reset link if the account exists."
    >
      <ForgotPasswordForm />
    </AuthShell>
  );
}
