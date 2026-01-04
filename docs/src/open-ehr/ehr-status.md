# OpenEHR EHR Status file

We have not added the `other_details` section yet (and we will have to see if we need to). However, below is an example of an EHR Status file in YAML format:

```yaml

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

other_details:
  items:
    - name:
        value: record_owner
      value:
        value: Gloucestershire Hospitals NHS Foundation Trust

    - name:
        value: confidentiality_level
      value:
        value: normal

    - name:
        value: created_reason
      value:
        value: Imported from legacy system

    - name:
        value: legal_status
      value:
        value: active


```
