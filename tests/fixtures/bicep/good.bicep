param location string = resourceGroup().location
param name string = 'st${uniqueString(resourceGroup().id)}'
module m './mod.bicep' = { name: 'm', params: { location: location } }
resource sa 'Microsoft.Storage/storageAccounts@2023-05-01' = {
  name: name
  location: location
  sku: { name: 'Standard_LRS' }
  kind: 'StorageV2'
}
output id string = sa.id
