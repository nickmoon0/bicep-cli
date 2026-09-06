param   location string=resourceGroup().location
resource sa   'Microsoft.Storage/storageAccounts@2023-05-01' = {
    name: 'stunformatted'
  location:location
  sku: {name: 'Standard_LRS'}
  kind: 'StorageV2'
}
