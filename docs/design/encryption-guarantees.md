# Encryption Guarantees Verification

Date: 2026-03-02

## Objective
Verify no plaintext persistence where encrypted storage is required.

## Guaranteed paths

### Mobile local chat storage
- Data persisted via SQLCipher (`sqflite_sqlcipher`) in encrypted SQLite DB.
- Encryption key material sourced from platform secure storage.

### Mobile backup artifact
- Backup data encrypted with AES-GCM (`cryptography` package).
- Secret key stored in secure storage and never written in plaintext backup blob.

## Verification approach
1. Code path inspection:
   - `mobile/lib/services/encrypted_database.dart`
   - `mobile/lib/services/encrypted_backup_service.dart`
2. Unit tests:
   - backup key handling and crypto helpers
3. Runtime checks:
   - Local backup file restore only via decryption path

## Constraints
- Full forensic binary-level DB validation is environment-specific and out of CI scope.
- Additional platform-native inspection can be run during release hardening.
