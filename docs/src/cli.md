# CLI

The VPR command-line interface (CLI) provides comprehensive tools for managing patient records, including demographics, clinical data, and care coordination.

## Usage

Inside the 'vpr-dev' Docker container or after building the `vpr-cli` crate:

```bash
vpr --help
```

## Available Commands

### Patient Management

- **`list`** - Lists all patients in the system
- **`initialise-full-record`** - Creates a complete patient record (demographics, clinical, and coordination repositories)

### Demographics

- **`initialise-demographics`** - Initialises a new demographics repository
- **`update-demographics`** - Updates demographic information (given names, last name, birth date)

### Clinical Records

- **`initialise-clinical`** - Initialises a new clinical repository
- **`write-ehr-status`** - Links clinical repository to demographics by writing EHR status file
- **`new-letter`** - Creates a new clinical letter with markdown content
- **`new-letter-with-attachments`** - Creates a new letter with file attachments
- **`read-letter`** - Reads and displays a clinical letter
- **`get-letter-attachments`** - Retrieves attachments for a letter

### Care Coordination

- **`initialise-coordination`** - Initialises a new coordination repository linked to clinical record
- **`create-thread`** - Creates a new messaging thread
- **`add-message`** - Adds a message to an existing thread
- **`read-communication`** - Reads a communication thread with all messages
- **`update-communication-ledger`** - Updates ledger (participants, status, visibility)
- **`update-coordination-status`** - Updates lifecycle status and flags

### Security

- **`create-certificate`** - Creates a professional registration certificate with X.509 encoding
- **`verify-clinical-commit-signature`** - Verifies cryptographic signature on latest clinical commit

### Development

- **`delete-all-data`** - **DEV ONLY**: Deletes all patient data (requires `DEV_ENV=true`)

## Common Options

### Author Registration

Many commands support professional registrations using the `--registration` flag, which can be repeated:

```bash
--registration "GMC" "1234567" --registration "NMC" "98765"
```

### Digital Signatures

Commands that modify records support optional digital signatures using the `--signature` flag:

```bash
--signature <ecdsa_private_key_pem>
```

The signature can be provided as PEM text, base64-encoded PEM, or a file path.

## Example Workflows

### Creating a Complete Patient Record

```bash
# 1. Create full record
vpr initialise-full-record "Emily" "Davis" "1985-03-20" \
  "Dr. Robert Brown" "robert.brown@example.com" "Clinician" "City Hospital"

# Outputs: Demographics UUID, Clinical UUID, Coordination UUID
```

### Adding a Letter

```bash
vpr new-letter <clinical_uuid> "Dr. Sarah Johnson" "sarah.johnson@example.com" \
  --role "Clinician" \
  --care-location "GP Clinic" \
  --content "# Clinical Note\n\nPatient assessment..."
```

### Adding a Letter with Attachments

```bash
vpr new-letter-with-attachments <clinical_uuid> \
  "Dr. Michael Chen" "michael.chen@example.com" \
  --role "Clinician" \
  --care-location "Hospital Laboratory" \
  --attachment-file "/path/to/lab_results.pdf"
```

### Creating a Communication Thread

```bash
vpr create-thread <coordination_uuid> "Dr. Brown" "brown@example.com" \
  --role "Clinician" \
  --care-location "City Hospital" \
  --participant "<clinical_uuid>" "clinician" "Dr. Brown" \
  --participant "<demographics_uuid>" "patient" "Emily Davis" \
  --initial-message "Initial consultation scheduled."
```

### Adding Messages to a Thread

```bash
vpr add-message <coordination_uuid> <thread_id> \
  "Nurse Wilson" "wilson@example.com" \
  --role "Clinician" \
  --care-location "City Hospital" \
  --message-type "clinician" \
  --message-body "Patient vitals recorded." \
  --message-author-id "<clinician_uuid>" \
  --message-author-name "Nurse Wilson"
```

## Getting Help

For detailed help on any command:

```bash
vpr <command> --help
```
