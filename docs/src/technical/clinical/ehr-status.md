# OpenEHR EHR Status file

Below is an example of an EHR Status file in YAML format. The actual implementation may or may not include the `other_details` section depending on use case:

```yaml
rm_version: rm_1_1_0

ehr_id:
  value: 1166765a-406a-4552-ac9b-8e141931a3dc

archetype_node_id: openEHR-EHR-STATUS.ehr_status.v1

name:
  value: EHR Status

subject:
  external_ref:
    id:
      value: 2db695ed-7cc0-4fc9-9b08-e0c738069b71
    namespace: vpr://mpi
    type: PERSON

is_queryable: true
is_modifiable: true
```

Note: The `other_details` field is optional and only included when additional metadata is needed for a specific use case.
